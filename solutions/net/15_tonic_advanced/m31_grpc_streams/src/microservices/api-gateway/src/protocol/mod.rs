use contracts::{Envelope, ErrorBody, Outcome, Reply, Response, decode, encode};

use crate::error::report;
use crate::routes::Answer;

pub(crate) fn contract(error: contracts::Error) -> ErrorBody {
    ErrorBody {
        code: error.code().to_string(),
        message: format!("запрос не разобран: {}", report(&error)),
    }
}

pub(crate) fn internal(message: &str) -> ErrorBody {
    ErrorBody {
        code: String::from("internal"),
        message: message.to_string(),
    }
}

pub(crate) fn request(text: &str) -> Result<Envelope, String> {
    decode::<Envelope>(text).map_err(|error| answer(None, Err(contract(error))))
}

pub(crate) fn answer(id: Option<u64>, answer: Answer) -> String {
    let outcome = match answer {
        Ok(reply) => Outcome::Ok(reply),
        Err(body) => Outcome::Error(body),
    };
    encode(&Response { id, outcome }).unwrap_or_else(|error| {
        let failed: Response<Reply> = Response {
            id,
            outcome: Outcome::Error(internal(&error.to_string())),
        };
        encode(&failed).unwrap_or_default()
    })
}
