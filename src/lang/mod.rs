mod enum_support;

#[macro_export]
macro_rules! enum_to_str {
    ($name:ident, $default:ident, $($var:ident,)+) => {
        use convert_case::{Case, Casing};
        use core::fmt::{Display, Formatter};

        #[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
        pub(crate) enum $name {
            $($var,)+
            $default,
        }

        impl<T: AsRef<str>> From<T> for $name {
            fn from(str: T) -> Self {
                let key = str.as_ref().to_case(Case::UpperCamel);
                match key.as_str() {
                    $(stringify!($var)=>$name::$var,)+
                    _ => $name::$default,
                }
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                let s = match self {
                    $($name::$var => stringify!($var).to_case(Case::Snake),)+
                    $name::$default => stringify!($default).to_case(Case::Snake),
                };
                write!(f, "{}", s)
            }
        }
    }
}