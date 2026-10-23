use contracts::{Envelope, ErrorBody, ErrorCode, Outcome, Response, Value, decode, encode};

use crate::common::report;
use crate::routes::Answer;

pub(crate) fn contract(error: contracts::Error) -> ErrorBody {
    let message = match &error {
        contracts::Error::BadEnvelope { .. } => String::from(
            "конверт не той формы: нужны id числом, service, method и data, meta по желанию",
        ),
        other => format!("запрос не разобран: {}", report(other)),
    };
    ErrorBody::new(error.kind(), message)
}

pub(crate) fn internal(message: &str) -> ErrorBody {
    tracing::warn!("api-gateway: {message}");
    ErrorBody::public(ErrorCode::Internal)
}

pub(crate) fn request(text: &str) -> Result<Envelope, String> {
    decode::<Envelope>(text).map_err(|error| answer(None, None, Err(contract(error))))
}

pub(crate) fn id_of(text: &str) -> Option<u64> {
    decode::<Value>(text).ok()?.get("id")?.as_u64()
}

pub(crate) fn answer(id: Option<u64>, rid: Option<&str>, answer: Answer) -> String {
    let outcome = match answer {
        Ok(value) => Outcome::Ok(value),
        Err(body) => match rid {
            Some(rid) => Outcome::Error(body.with_request_id(rid)),
            None => Outcome::Error(body),
        },
    };
    let mut response = Response::new(id, outcome);
    if let Some(rid) = rid {
        response = response.with_request_id(rid);
    }
    encode(&response).unwrap_or_else(|error| {
        let failed: Response<Value> =
            Response::new(id, Outcome::Error(internal(&error.to_string())));
        encode(&failed).unwrap_or_default()
    })
}
