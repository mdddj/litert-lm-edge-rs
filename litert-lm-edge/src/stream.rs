use crate::input::OwnedInputs;
use crate::{Error, Result};
use litert_lm_edge_sys as ffi;
use std::ffi::{c_char, c_void, CStr};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Chunk(String),
    Final,
    Error(String),
}

pub struct TextStream<'session> {
    receiver: Receiver<StreamEvent>,
    state: Option<NonNull<StreamState>>,
    session: NonNull<ffi::LiteRtLmSession>,
    terminal_received: bool,
    _session: PhantomData<&'session mut ffi::LiteRtLmSession>,
}

struct StreamState {
    sender: Sender<StreamEvent>,
    input: OwnedInputs,
}

pub(crate) fn start_text_stream<'session>(
    session: NonNull<ffi::LiteRtLmSession>,
    input: OwnedInputs,
    _session_lifetime: PhantomData<&'session mut ffi::LiteRtLmSession>,
) -> Result<TextStream<'session>> {
    let (sender, receiver) = mpsc::channel();
    let state = Box::new(StreamState { sender, input });
    let state = NonNull::new(Box::into_raw(state)).expect("Box::into_raw never returns null");

    // SAFETY: session is provided by Session and is valid for this call. The input data and
    // callback state are owned by StreamState and remain allocated until TextStream is dropped
    // after receiving Final/Error or until stream start fails.
    let code = unsafe {
        let state_ref = state.as_ref();
        ffi::litert_lm_session_generate_content_stream(
            session.as_ptr(),
            state_ref.input.as_ffi().as_ptr(),
            state_ref.input.as_ffi().len(),
            Some(stream_callback),
            state.as_ptr().cast(),
        )
    };

    if code != 0 {
        // SAFETY: ownership is still with this function because LiteRT-LM failed to start the
        // stream and will not call the callback.
        unsafe { drop(Box::from_raw(state.as_ptr())) };
        return Err(Error::StartStream(code));
    }

    Ok(TextStream {
        receiver,
        state: Some(state),
        session,
        terminal_received: false,
        _session: PhantomData,
    })
}

impl Iterator for TextStream<'_> {
    type Item = StreamEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.receiver.recv().ok()?;
        self.mark_terminal(&event);
        Some(event)
    }
}

impl Drop for TextStream<'_> {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };

        if !self.terminal_received {
            // SAFETY: session is borrowed for the lifetime of TextStream and remains valid while
            // TextStream is being dropped.
            unsafe { ffi::litert_lm_session_cancel_process(self.session.as_ptr()) };

            if !self.drain_until_terminal(Duration::from_secs(30)) {
                // The native runtime may still invoke the callback. Leaking this small state is
                // safer than freeing callback_data while a foreign background thread may use it.
                return;
            }
        }

        // SAFETY: state was allocated with Box::into_raw in start_text_stream. We only free it
        // after observing a terminal event from the native stream, so LiteRT-LM should not invoke
        // the callback again.
        unsafe { drop(Box::from_raw(state.as_ptr())) };
    }
}

impl TextStream<'_> {
    fn mark_terminal(&mut self, event: &StreamEvent) {
        if matches!(event, StreamEvent::Final | StreamEvent::Error(_)) {
            self.terminal_received = true;
        }
    }

    fn drain_until_terminal(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while !self.terminal_received {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }

            match self.receiver.recv_timeout(deadline - now) {
                Ok(event) => self.mark_terminal(&event),
                Err(mpsc::RecvTimeoutError::Timeout) => return false,
                Err(mpsc::RecvTimeoutError::Disconnected) => return true,
            }
        }

        true
    }
}

unsafe extern "C" fn stream_callback(
    callback_data: *mut c_void,
    chunk: *const c_char,
    is_final: bool,
    error_msg: *const c_char,
) {
    if callback_data.is_null() {
        return;
    }

    // SAFETY: callback_data is the StreamState pointer provided to LiteRT-LM when starting the
    // stream. LiteRT-LM invokes this callback while the stream is active.
    let state = callback_data.cast::<StreamState>();

    if !error_msg.is_null() {
        // SAFETY: error_msg is documented as a null-terminated string valid for this callback.
        let message = unsafe { CStr::from_ptr(error_msg) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: state is the callback data pointer supplied by start_text_stream and remains
        // allocated until a terminal event is observed, or intentionally leaked on early timeout.
        let _ = unsafe { (*state).sender.send(StreamEvent::Error(message)) };
        return;
    }

    if !chunk.is_null() {
        // SAFETY: chunk is documented as a null-terminated string valid only for this callback;
        // it is copied immediately into an owned String.
        let text = unsafe { CStr::from_ptr(chunk) }
            .to_string_lossy()
            .into_owned();
        if !text.is_empty() {
            // SAFETY: state is the callback data pointer supplied by start_text_stream and remains
            // allocated while the native stream is active.
            let _ = unsafe { (*state).sender.send(StreamEvent::Chunk(text)) };
        }
    }

    if is_final {
        // SAFETY: state is the callback data pointer supplied by start_text_stream and remains
        // allocated until this terminal event is observed by TextStream.
        let _ = unsafe { (*state).sender.send(StreamEvent::Final) };
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::ffi::CString;

    fn callback_for_test(
        state: &mut StreamState,
        chunk: Option<&str>,
        is_final: bool,
        error: Option<&str>,
    ) {
        let chunk = chunk.map(CString::new).transpose().unwrap();
        let error = error.map(CString::new).transpose().unwrap();
        // SAFETY: state is a valid StreamState for this test and CString pointers live for the
        // duration of the callback call.
        unsafe {
            stream_callback(
                (state as *mut StreamState).cast(),
                chunk
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                is_final,
                error
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            );
        }
    }

    #[test]
    fn callback_sends_chunk_and_final() {
        let (sender, receiver) = mpsc::channel();
        let input = OwnedInputs::new(&[crate::InputData::Text("hello".to_owned())]).unwrap();
        let mut state = StreamState { sender, input };

        callback_for_test(&mut state, Some("a"), false, None);
        callback_for_test(&mut state, Some("b"), true, None);

        assert_eq!(receiver.recv().unwrap(), StreamEvent::Chunk("a".to_owned()));
        assert_eq!(receiver.recv().unwrap(), StreamEvent::Chunk("b".to_owned()));
        assert_eq!(receiver.recv().unwrap(), StreamEvent::Final);
    }

    #[test]
    fn callback_sends_error() {
        let (sender, receiver) = mpsc::channel();
        let input = OwnedInputs::new(&[crate::InputData::Text("hello".to_owned())]).unwrap();
        let mut state = StreamState { sender, input };

        callback_for_test(&mut state, None, false, Some("failed"));

        assert_eq!(
            receiver.recv().unwrap(),
            StreamEvent::Error("failed".to_owned())
        );
    }
}
