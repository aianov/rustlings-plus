use nats_engine::js::{Published, StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusError, Meta, Pattern, Subject, token};

use crate::job::Job;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSpec {
    pub stream: String,
    pub prefix: Vec<String>,
    pub dead_stream: String,
    pub dead_prefix: Vec<String>,
}

#[derive(Clone)]
pub struct Queue {
    bus: Bus,
    spec: QueueSpec,
}

fn encode(error: impl std::fmt::Display) -> BusError {
    BusError::Encode(error.to_string())
}

fn joined(prefix: &[String], tail: &str) -> Vec<String> {
    let mut words = prefix.to_vec();
    words.push(tail.to_string());
    words
}

fn subject(words: &[String]) -> Result<Subject, BusError> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    Subject::new(&words).map_err(encode)
}

impl Queue {
    pub async fn open(bus: Bus, spec: QueueSpec) -> Result<Queue, BusError> {
        let all = joined(&spec.prefix, ">").join(".");
        let work = StreamSpec::new(&spec.stream, vec![all]).work_queue();
        ensure_stream(&bus, &work).await?;
        let buried = joined(&spec.dead_prefix, ">").join(".");
        ensure_stream(&bus, &StreamSpec::new(&spec.dead_stream, vec![buried])).await?;
        Ok(Queue { bus, spec })
    }

    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    pub fn spec(&self) -> &QueueSpec {
        &self.spec
    }

    pub fn subject(&self, kind: &str) -> Result<Subject, BusError> {
        token(kind).map_err(encode)?;
        subject(&joined(&self.spec.prefix, kind))
    }

    pub fn filter(&self, kind: Option<&str>) -> Result<Pattern, BusError> {
        let words = joined(&self.spec.prefix, kind.unwrap_or(">"));
        Pattern::parse(&words.join(".")).map_err(encode)
    }

    pub fn dead(&self, kind: &str) -> Result<Subject, BusError> {
        let kind = if token(kind).is_ok() {
            kind
        } else {
            "unreadable"
        };
        subject(&joined(&self.spec.dead_prefix, kind))
    }

    pub async fn enqueue(&self, job: &Job) -> Result<Published, BusError> {
        let subject = self.subject(&job.kind)?;
        publish_event(&self.bus, &subject, &job.id, job, &Meta::fresh()).await
    }
}
