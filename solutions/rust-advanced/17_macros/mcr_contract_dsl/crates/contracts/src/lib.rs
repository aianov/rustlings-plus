/// Одна запись таблицы маршрутов сервиса.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub name: &'static str,
    pub request: &'static str,
    pub response: &'static str,
}

/// Имена, которые встречаются в списке больше одного раза.
pub fn find_duplicates(names: &[&'static str]) -> Vec<&'static str> {
    let mut seen: Vec<&'static str> = Vec::new();
    let mut duplicates: Vec<&'static str> = Vec::new();
    for name in names {
        if seen.contains(name) {
            if !duplicates.contains(name) {
                duplicates.push(name);
            }
        } else {
            seen.push(name);
        }
    }
    duplicates
}

#[macro_export]
macro_rules! service {
    (
        $(#[$meta:meta])*
        $vis:vis enum $enum:ident {
            $( $variant:ident = $name:literal ( $request:ty ) -> $response:ty ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $enum {
            $( $variant, )+
        }

        impl $enum {
            pub const ALL: &'static [$enum] = &[$( $enum::$variant ),+];

            pub const ROUTES: &'static [$crate::Route] = &[
                $( $crate::Route { name: $name, request: stringify!($request), response: stringify!($response) } ),+
            ];

            pub fn name(self) -> &'static str {
                match self {
                    $( $enum::$variant => $name, )+
                }
            }

            pub fn request_type(self) -> &'static str {
                match self {
                    $( $enum::$variant => stringify!($request), )+
                }
            }

            pub fn response_type(self) -> &'static str {
                match self {
                    $( $enum::$variant => stringify!($response), )+
                }
            }

            pub fn from_name(name: &str) -> Option<$enum> {
                match name {
                    $( $name => Some($enum::$variant), )+
                    _ => None,
                }
            }

            pub fn duplicates() -> Vec<&'static str> {
                $crate::find_duplicates(&[$( $name ),+])
            }
        }

        const _: fn() = || {
            fn exists<T: ?Sized>() {}
            $(
                exists::<$request>();
                exists::<$response>();
            )+
        };
    };
}
