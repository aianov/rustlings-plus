use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub trait Notifier: Send + Sync {
    fn notify(&self, user: &str, text: &str);
}

pub trait Layer: Send + Sync {
    fn before(&self, command: &str, args: &str) -> Option<Response>;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response { status: 200, body: self }
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response { status: 200, body: self.to_string() }
    }
}

impl IntoResponse for (u16, String) {
    fn into_response(self) -> Response {
        Response { status: self.0, body: self.1 }
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(error) => {
                let mut response = error.into_response();
                if response.status == 200 {
                    response.status = 500;
                }
                response
            }
        }
    }
}

#[derive(Default)]
pub struct Context {
    ext: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Context {
    pub fn new() -> Context {
        Context::default()
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) -> Option<T> {
        self.ext
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| (old as Box<dyn Any>).downcast::<T>().ok())
            .map(|old| *old)
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.ext.get(&TypeId::of::<T>()).and_then(|value| value.downcast_ref::<T>())
    }

    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.ext.get_mut(&TypeId::of::<T>()).and_then(|value| value.downcast_mut::<T>())
    }
}

trait ErasedHandler: Send + Sync {
    fn call(&self, ctx: &mut Context, args: &str) -> Response;
}

struct FnHandler<F> {
    f: F,
}

impl<F, R> ErasedHandler for FnHandler<F>
where
    F: Fn(&mut Context, &str) -> R + Send + Sync,
    R: IntoResponse,
{
    fn call(&self, ctx: &mut Context, args: &str) -> Response {
        (self.f)(ctx, args).into_response()
    }
}

pub struct Bus {
    handlers: HashMap<String, Box<dyn ErasedHandler>>,
    layers: Vec<Box<dyn Layer>>,
}

impl Default for Bus {
    fn default() -> Self {
        Bus::new()
    }
}

impl Bus {
    pub fn new() -> Bus {
        Bus { handlers: HashMap::new(), layers: Vec::new() }
    }

    pub fn register<F, R>(&mut self, name: &str, handler: F) -> bool
    where
        F: Fn(&mut Context, &str) -> R + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        self.handlers.insert(name.to_string(), Box::new(FnHandler { f: handler })).is_some()
    }

    pub fn layer(&mut self, layer: impl Layer + 'static) {
        self.layers.push(Box::new(layer));
    }

    pub fn dispatch(&self, name: &str, ctx: &mut Context, args: &str) -> Response {
        if let Some(response) = self.layers.iter().find_map(|layer| layer.before(name, args)) {
            return response;
        }
        match self.handlers.get(name) {
            Some(handler) => handler.call(ctx, args),
            None => Response { status: 404, body: format!("unknown command: {name}") },
        }
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.handlers.keys().cloned().collect();
        names.sort_unstable();
        names
    }
}
