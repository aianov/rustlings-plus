use contracts::{
    Envelope, ErrorBody, Outcome, Reply, Response, SessionCall, UserView, decode, decode_call,
    decode_session, encode, needs_actor,
};
use social_core::serve;
use tokio::sync::mpsc;

use crate::hub::Link;
use crate::server::Shared;
use crate::session::Session;

pub(crate) struct State {
    session: Session,
    link: Option<Link>,
    outbox: mpsc::Sender<String>,
}

impl State {
    pub(crate) fn new(outbox: mpsc::Sender<String>) -> State {
        State {
            session: Session::Anonymous,
            link: None,
            outbox,
        }
    }
}

pub(crate) async fn handle(shared: &Shared, state: &mut State, line: &str) -> String {
    let response = match decode::<Envelope>(line) {
        Ok(envelope) => {
            let id = Some(envelope.id);
            let outcome = match dispatch(shared, state, envelope).await {
                Ok(reply) => Outcome::Ok(reply),
                Err(body) => Outcome::Error(body),
            };
            Response { id, outcome }
        }
        Err(error) => Response {
            id: None,
            outcome: Outcome::Error(contract(error)),
        },
    };
    encode(&response).unwrap_or_else(|error| failure("internal", &error))
}

pub(crate) fn failure(code: &str, error: &dyn std::fmt::Display) -> String {
    let response: Response<Reply> = Response {
        id: None,
        outcome: Outcome::Error(ErrorBody {
            code: code.to_string(),
            message: error.to_string(),
        }),
    };
    encode(&response).unwrap_or_default()
}

fn contract(error: contracts::Error) -> ErrorBody {
    social_core::Error::from(error).to_body()
}

async fn dispatch(
    shared: &Shared,
    state: &mut State,
    mut envelope: Envelope,
) -> Result<Reply, ErrorBody> {
    if envelope.service == "session" || envelope.method.starts_with("session.") {
        let call = decode_session(&envelope).map_err(contract)?;
        return session(shared, state, call).await;
    }
    if needs_actor(&envelope.method) {
        let user = state.session.actor().map_err(|error| error.to_body())?;
        envelope.act_as(user).map_err(contract)?;
    }
    let call = decode_call(&envelope).map_err(contract)?;
    serve(&shared.app, &call)
        .await
        .map_err(|error| error.to_body())
}

async fn session(
    shared: &Shared,
    state: &mut State,
    call: SessionCall,
) -> Result<Reply, ErrorBody> {
    match call {
        SessionCall::Hello { username } => {
            let name = username.as_ref();
            let Some(user) = shared.app.find_user(name).await else {
                return Err(social_core::Error::UnknownName(name.to_string()).to_body());
            };
            let me = UserView {
                id: user.id(),
                username: user.name().to_string(),
            };
            state.session = state
                .session
                .hello(me.clone())
                .map_err(|error| error.to_body())?;
            state.link = Some(shared.hub.join(me.id, state.outbox.clone()));
            Ok(Reply::User(me))
        }
        SessionCall::Bye => {
            let (next, me) = state.session.bye().map_err(|error| error.to_body())?;
            state.session = next;
            state.link = None;
            Ok(Reply::User(me))
        }
    }
}
