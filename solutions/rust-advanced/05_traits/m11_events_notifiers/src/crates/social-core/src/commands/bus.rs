use std::collections::HashMap;

use contracts::ParseError;

use super::{CommandError, Context, IntoResponse, Response};

trait Handler: Send + Sync {
    fn call(&self, ctx: &mut Context, line: &str) -> Response;
}

struct FnHandler<F> {
    f: F,
}

impl<F, R> Handler for FnHandler<F>
where
    F: Fn(&mut Context, &str) -> R + Send + Sync,
    R: IntoResponse,
{
    fn call(&self, ctx: &mut Context, line: &str) -> Response {
        (self.f)(ctx, line).into_response()
    }
}

#[derive(Default)]
pub struct CommandBus {
    handlers: HashMap<String, Box<dyn Handler>>,
}

impl CommandBus {
    pub fn new() -> CommandBus {
        CommandBus::default()
    }

    pub fn register<F, R>(&mut self, name: &str, handler: F) -> bool
    where
        F: Fn(&mut Context, &str) -> R + Send + Sync + 'static,
        R: IntoResponse,
    {
        self.handlers
            .insert(name.to_string(), Box::new(FnHandler { f: handler }))
            .is_some()
    }

    pub fn dispatch(&self, ctx: &mut Context, line: &str) -> Response {
        let line = line.trim();
        let Some(name) = line.split_whitespace().next() else {
            return CommandError::Parse(ParseError::Empty).into_response();
        };
        match self.handlers.get(name) {
            Some(handler) => handler.call(ctx, line),
            None => {
                CommandError::Parse(ParseError::UnknownCommand(name.to_string())).into_response()
            }
        }
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.handlers.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}
