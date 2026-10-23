
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Malformed,
    ValidationFailed,
    Unauthenticated,
    Forbidden,
    NotFound,
    Conflict,
    RateLimited,
    Internal,
    DependencyDown,
    DeadlineExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GrpcCode {
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    Internal = 13,
    Unavailable = 14,
    Unauthenticated = 16,
    InvalidArgument = 3,
}

pub const ALL: [Failure; 10] = [
    Failure::Malformed,
    Failure::ValidationFailed,
    Failure::Unauthenticated,
    Failure::Forbidden,
    Failure::NotFound,
    Failure::Conflict,
    Failure::RateLimited,
    Failure::Internal,
    Failure::DependencyDown,
    Failure::DeadlineExceeded,
];

pub fn http_status(failure: Failure) -> u16 {
    match failure {
        Failure::Malformed => 400,
        Failure::ValidationFailed => 422,
        Failure::Unauthenticated => 401,
        Failure::Forbidden => 403,
        Failure::NotFound => 404,
        Failure::Conflict => 409,
        Failure::RateLimited => 429,
        Failure::Internal => 500,
        Failure::DependencyDown => 503,
        Failure::DeadlineExceeded => 504,
    }
}

pub fn grpc_code(failure: Failure) -> GrpcCode {
    match failure {
        Failure::Malformed | Failure::ValidationFailed => GrpcCode::InvalidArgument,
        Failure::Unauthenticated => GrpcCode::Unauthenticated,
        Failure::Forbidden => GrpcCode::PermissionDenied,
        Failure::NotFound => GrpcCode::NotFound,
        Failure::Conflict => GrpcCode::AlreadyExists,
        Failure::RateLimited => GrpcCode::ResourceExhausted,
        Failure::Internal => GrpcCode::Internal,
        Failure::DependencyDown => GrpcCode::Unavailable,
        Failure::DeadlineExceeded => GrpcCode::DeadlineExceeded,
    }
}

pub fn is_client_fault(failure: Failure) -> bool {
    http_status(failure) < 500
}

pub fn retryable(failure: Failure) -> bool {
    matches!(failure, Failure::RateLimited | Failure::DependencyDown | Failure::DeadlineExceeded)
}

fn main() {
    for failure in ALL {
        println!("{failure:?}: {} / {:?}", http_status(failure), grpc_code(failure));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_table() {
        let expected = [
            (Failure::Malformed, 400),
            (Failure::ValidationFailed, 422),
            (Failure::Unauthenticated, 401),
            (Failure::Forbidden, 403),
            (Failure::NotFound, 404),
            (Failure::Conflict, 409),
            (Failure::RateLimited, 429),
            (Failure::Internal, 500),
            (Failure::DependencyDown, 503),
            (Failure::DeadlineExceeded, 504),
        ];
        for (failure, status) in expected {
            assert_eq!(http_status(failure), status, "{failure:?}: не тот HTTP-статус");
        }
    }

    #[test]
    fn grpc_table() {
        let expected = [
            (Failure::Malformed, 3),
            (Failure::ValidationFailed, 3),
            (Failure::Unauthenticated, 16),
            (Failure::Forbidden, 7),
            (Failure::NotFound, 5),
            (Failure::Conflict, 6),
            (Failure::RateLimited, 8),
            (Failure::Internal, 13),
            (Failure::DependencyDown, 14),
            (Failure::DeadlineExceeded, 4),
        ];
        for (failure, code) in expected {
            assert_eq!(grpc_code(failure) as i32, code, "{failure:?}: не тот числовой gRPC-код");
        }
    }

    #[test]
    fn who_is_to_blame() {
        for failure in ALL {
            let status = http_status(failure);
            assert_eq!(
                is_client_fault(failure),
                (400..500).contains(&status),
                "{failure:?}: виноват клиент ровно тогда, когда статус 4xx"
            );
        }
        assert!(!is_client_fault(Failure::DependencyDown), "упавшая база не вина клиента");
        assert!(is_client_fault(Failure::Forbidden), "чужой ресурс это вина клиента");
    }

    #[test]
    fn only_temporary_failures_are_worth_repeating() {
        for failure in [Failure::RateLimited, Failure::DependencyDown, Failure::DeadlineExceeded] {
            assert!(retryable(failure), "{failure:?}: через время тот же запрос может пройти");
        }
        for failure in [
            Failure::Malformed,
            Failure::ValidationFailed,
            Failure::Unauthenticated,
            Failure::Forbidden,
            Failure::NotFound,
            Failure::Conflict,
            Failure::Internal,
        ] {
            assert!(!retryable(failure), "{failure:?}: повтор того же запроса даст тот же ответ или сделает хуже");
        }
    }
}
