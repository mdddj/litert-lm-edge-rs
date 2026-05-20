use crate::{
    ConversationConfig, Engine, EngineBuilder, Error, InputData, Message, Result, SendOptions,
    SessionConfig, StreamEvent,
};
use serde_json::Value;
use std::thread::{self, JoinHandle};
use tokio::sync::{mpsc, oneshot};

pub struct TokioEngine {
    requests: std::sync::mpsc::Sender<EngineRequest>,
    _worker: JoinHandle<()>,
}

pub struct TokioSession {
    requests: std::sync::mpsc::Sender<EngineRequest>,
    id: u64,
}

pub struct TokioConversation {
    requests: std::sync::mpsc::Sender<EngineRequest>,
    id: u64,
}

pub struct TokioTextStream {
    receiver: mpsc::UnboundedReceiver<StreamEvent>,
}

enum EngineRequest {
    CreateSession {
        config: SessionConfig,
        respond_to: oneshot::Sender<Result<u64>>,
    },
    SessionGenerateContent {
        id: u64,
        inputs: Vec<InputData>,
        respond_to: oneshot::Sender<Result<String>>,
    },
    SessionGenerateContentStream {
        id: u64,
        inputs: Vec<InputData>,
        events: mpsc::UnboundedSender<StreamEvent>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    SessionDelete {
        id: u64,
    },
    CreateConversation {
        config: ConversationConfig,
        respond_to: oneshot::Sender<Result<u64>>,
    },
    ConversationSendMessage {
        id: u64,
        message: Message,
        options: SendOptions,
        respond_to: oneshot::Sender<Result<Message>>,
    },
    ConversationSendRaw {
        id: u64,
        message: Value,
        options: SendOptions,
        respond_to: oneshot::Sender<Result<Value>>,
    },
    ConversationDelete {
        id: u64,
    },
}

impl TokioEngine {
    pub async fn new(builder: EngineBuilder) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (initialized, initialized_response) = oneshot::channel();
        let worker = thread::Builder::new()
            .name("litert-lm-edge-tokio".to_owned())
            .spawn(move || match builder.build() {
                Ok(engine) => {
                    let _ = initialized.send(Ok(()));
                    worker_loop(engine, receiver);
                }
                Err(error) => {
                    let _ = initialized.send(Err(error));
                }
            })
            .map_err(Error::Io)?;

        match initialized_response
            .await
            .map_err(|_| Error::WorkerStopped)?
        {
            Ok(()) => {}
            Err(error) => {
                let _ = worker.join();
                return Err(error);
            }
        }

        Ok(Self {
            requests: sender,
            _worker: worker,
        })
    }

    pub async fn create_session(&self, config: SessionConfig) -> Result<TokioSession> {
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::CreateSession { config, respond_to })?;
        let id = response.await.map_err(|_| Error::WorkerStopped)??;
        Ok(TokioSession {
            requests: self.requests.clone(),
            id,
        })
    }

    pub async fn create_conversation(
        &self,
        config: ConversationConfig,
    ) -> Result<TokioConversation> {
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::CreateConversation { config, respond_to })?;
        let id = response.await.map_err(|_| Error::WorkerStopped)??;
        Ok(TokioConversation {
            requests: self.requests.clone(),
            id,
        })
    }

    fn send(&self, request: EngineRequest) -> Result<()> {
        self.requests
            .send(request)
            .map_err(|_| Error::WorkerStopped)
    }
}

impl TokioSession {
    pub async fn generate_text(&self, prompt: impl Into<String>) -> Result<String> {
        self.generate_content(vec![InputData::Text(prompt.into())])
            .await
    }

    pub async fn generate_content(&self, inputs: Vec<InputData>) -> Result<String> {
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::SessionGenerateContent {
            id: self.id,
            inputs,
            respond_to,
        })?;
        response.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn generate_text_stream(&self, prompt: impl Into<String>) -> Result<TokioTextStream> {
        self.generate_content_stream(vec![InputData::Text(prompt.into())])
            .await
    }

    pub async fn generate_content_stream(&self, inputs: Vec<InputData>) -> Result<TokioTextStream> {
        let (events, receiver) = mpsc::unbounded_channel();
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::SessionGenerateContentStream {
            id: self.id,
            inputs,
            events,
            respond_to,
        })?;
        response.await.map_err(|_| Error::WorkerStopped)??;

        Ok(TokioTextStream { receiver })
    }

    fn send(&self, request: EngineRequest) -> Result<()> {
        self.requests
            .send(request)
            .map_err(|_| Error::WorkerStopped)
    }
}

impl Drop for TokioSession {
    fn drop(&mut self) {
        let _ = self
            .requests
            .send(EngineRequest::SessionDelete { id: self.id });
    }
}

impl TokioConversation {
    pub async fn send_message(&self, message: Message) -> Result<Message> {
        self.send_message_with_options(message, SendOptions::default())
            .await
    }

    pub async fn send_message_with_options(
        &self,
        message: Message,
        options: SendOptions,
    ) -> Result<Message> {
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::ConversationSendMessage {
            id: self.id,
            message,
            options,
            respond_to,
        })?;
        response.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn send_message_raw(&self, message: Value, options: SendOptions) -> Result<Value> {
        let (respond_to, response) = oneshot::channel();
        self.send(EngineRequest::ConversationSendRaw {
            id: self.id,
            message,
            options,
            respond_to,
        })?;
        response.await.map_err(|_| Error::WorkerStopped)?
    }

    fn send(&self, request: EngineRequest) -> Result<()> {
        self.requests
            .send(request)
            .map_err(|_| Error::WorkerStopped)
    }
}

impl Drop for TokioConversation {
    fn drop(&mut self) {
        let _ = self
            .requests
            .send(EngineRequest::ConversationDelete { id: self.id });
    }
}

impl TokioTextStream {
    pub async fn next(&mut self) -> Option<StreamEvent> {
        self.receiver.recv().await
    }
}

fn worker_loop(engine: Engine, receiver: std::sync::mpsc::Receiver<EngineRequest>) {
    let mut state = WorkerState::new(engine);
    for request in receiver {
        state.handle(request);
    }
}

struct WorkerState {
    sessions: Vec<(u64, crate::Session<'static>)>,
    conversations: Vec<(u64, crate::Conversation<'static>)>,
    engine: Engine,
    next_id: u64,
}

impl WorkerState {
    fn new(engine: Engine) -> Self {
        Self {
            sessions: Vec::new(),
            conversations: Vec::new(),
            engine,
            next_id: 1,
        }
    }

    fn handle(&mut self, request: EngineRequest) {
        match request {
            EngineRequest::CreateSession { config, respond_to } => {
                let result = self.create_session(config);
                let _ = respond_to.send(result);
            }
            EngineRequest::SessionGenerateContent {
                id,
                inputs,
                respond_to,
            } => {
                let result = self
                    .session_mut(id)
                    .ok_or(Error::InvalidResponse("unknown async session".to_owned()))
                    .and_then(|session| session.generate_content(&inputs));
                let _ = respond_to.send(result);
            }
            EngineRequest::SessionGenerateContentStream {
                id,
                inputs,
                events,
                respond_to,
            } => {
                self.session_stream(id, inputs, events, respond_to);
            }
            EngineRequest::SessionDelete { id } => {
                remove_by_id(&mut self.sessions, id);
            }
            EngineRequest::CreateConversation { config, respond_to } => {
                let result = self.create_conversation(config);
                let _ = respond_to.send(result);
            }
            EngineRequest::ConversationSendMessage {
                id,
                message,
                options,
                respond_to,
            } => {
                let result = self
                    .conversation_mut(id)
                    .ok_or(Error::InvalidResponse(
                        "unknown async conversation".to_owned(),
                    ))
                    .and_then(|conversation| {
                        conversation.send_message_with_options(message, options)
                    });
                let _ = respond_to.send(result);
            }
            EngineRequest::ConversationSendRaw {
                id,
                message,
                options,
                respond_to,
            } => {
                let result = self
                    .conversation_mut(id)
                    .ok_or(Error::InvalidResponse(
                        "unknown async conversation".to_owned(),
                    ))
                    .and_then(|conversation| conversation.send_message_raw(message, options));
                let _ = respond_to.send(result);
            }
            EngineRequest::ConversationDelete { id } => {
                remove_by_id(&mut self.conversations, id);
            }
        }
    }

    fn create_session(&mut self, config: SessionConfig) -> Result<u64> {
        let id = self.allocate_id();
        let session = self.engine.create_session(config)?;
        // SAFETY: WorkerState owns engine and all stored sessions. Sessions are dropped before
        // engine because fields drop in declaration order and engine is declared first, so it
        // drops last. The worker never moves sessions outside this thread.
        let session =
            unsafe { std::mem::transmute::<crate::Session<'_>, crate::Session<'static>>(session) };
        self.sessions.push((id, session));
        Ok(id)
    }

    fn create_conversation(&mut self, config: ConversationConfig) -> Result<u64> {
        let id = self.allocate_id();
        let conversation = self.engine.create_conversation(config)?;
        // SAFETY: WorkerState owns engine and all stored conversations. Conversations are dropped
        // before engine because fields drop in declaration order and engine is declared first, so
        // it drops last. The worker never moves conversations outside this thread.
        let conversation = unsafe {
            std::mem::transmute::<crate::Conversation<'_>, crate::Conversation<'static>>(
                conversation,
            )
        };
        self.conversations.push((id, conversation));
        Ok(id)
    }

    fn session_stream(
        &mut self,
        id: u64,
        inputs: Vec<InputData>,
        events: mpsc::UnboundedSender<StreamEvent>,
        respond_to: oneshot::Sender<Result<()>>,
    ) {
        let Some(session) = self.session_mut(id) else {
            let _ = respond_to.send(Err(Error::InvalidResponse(
                "unknown async session".to_owned(),
            )));
            return;
        };

        let stream = match session.generate_content_stream(&inputs) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = respond_to.send(Err(error));
                return;
            }
        };

        let _ = respond_to.send(Ok(()));
        for event in stream {
            let terminal = matches!(event, StreamEvent::Final | StreamEvent::Error(_));
            let send_failed = events.send(event).is_err();
            if terminal || send_failed {
                break;
            }
        }
    }

    fn session_mut(&mut self, id: u64) -> Option<&mut crate::Session<'static>> {
        self.sessions
            .iter_mut()
            .find_map(|(session_id, session)| (*session_id == id).then_some(session))
    }

    fn conversation_mut(&mut self, id: u64) -> Option<&mut crate::Conversation<'static>> {
        self.conversations
            .iter_mut()
            .find_map(|(conversation_id, conversation)| {
                (*conversation_id == id).then_some(conversation)
            })
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        self.conversations.clear();
        self.sessions.clear();
    }
}

fn remove_by_id<T>(items: &mut Vec<(u64, T)>, id: u64) {
    if let Some(index) = items.iter().position(|(item_id, _)| *item_id == id) {
        items.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_by_id_removes_matching_item() {
        let mut items = vec![(1, "a"), (2, "b"), (3, "c")];
        remove_by_id(&mut items, 2);
        assert_eq!(items, vec![(1, "a"), (3, "c")]);
    }
}
