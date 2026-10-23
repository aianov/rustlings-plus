use crate::Body;
use futures::StreamExt;
use std::io;

#[derive(Debug, PartialEq)]
pub struct TooLarge {
    pub limit: u64,
}

pub fn limit(body: Body, max: u64, declared: Option<u64>) -> Result<Body, TooLarge> {
    if declared.is_some_and(|d| d > max) {
        return Err(TooLarge { limit: max });
    }
    let state = (body, 0u64, false);
    let stream = futures::stream::unfold(state, move |(mut body, seen, done)| async move {
        if done {
            return None;
        }
        match body.next().await? {
            Ok(chunk) => {
                let seen = seen + chunk.len() as u64;
                if seen > max {
                    let err = io::Error::new(io::ErrorKind::FileTooLarge, format!("тело больше {max} байт"));
                    Some((Err(err), (body, seen, true)))
                } else {
                    Some((Ok(chunk), (body, seen, false)))
                }
            }
            Err(e) => Some((Err(e), (body, seen, true))),
        }
    });
    Ok(stream.boxed())
}
