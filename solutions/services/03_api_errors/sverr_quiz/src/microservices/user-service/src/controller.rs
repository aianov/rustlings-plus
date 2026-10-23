use crate::domain::{UserError, UserStore};
use contracts::api::{User, UserApi};
use contracts::errors::{ApiError, Code, GrpcStatus, grpc_status};

pub struct UserController {
    store: UserStore,
}

impl UserController {
    pub fn new(store: UserStore) -> UserController {
        UserController { store }
    }
}

pub fn to_api(err: &UserError) -> ApiError {
    match err {
        UserError::NotFound(id) => ApiError::new(Code::UserNotFound).with_message(&format!("пользователь {id} не найден")),
        UserError::NameTaken => ApiError::new(Code::UsernameTaken),
        UserError::Storage(_) => ApiError::new(Code::Internal),
    }
}

impl UserApi for UserController {
    fn get(&self, id: u64, request_id: &str) -> Result<User, GrpcStatus> {
        self.store.get(id).map_err(|err| grpc_status(&to_api(&err), request_id))
    }
}
