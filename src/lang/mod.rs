#[macro_export]
macro_rules! enum_to_str {
    ($vis:vis $name:ident, $default:ident, $($var:ident,)+) => {
        use convert_case::{Case, Casing};
        use core::fmt::{Display, Formatter};
        use serde::{Serialize, Deserialize};

        #[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
        $vis enum $name {
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

#[cfg(test)]
mod test {
    enum_to_str!(
        pub Color, White, Red, Green,
    );

    #[test]
    fn test_enum_to_str() {
        let values = vec![
            ("white", Color::White),
            ("GREEN", Color::Green),
            ("Red", Color::Red),
            ("asdf", Color::White),
        ];
        for (v, expected) in values {
            assert_eq!(expected, Color::from(v));
        }
    }
}
