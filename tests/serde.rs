use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8};

// mod atomic_u8_serde {
//     use serde::{Deserialize, Deserializer, Serializer};
//     use std::sync::atomic::{AtomicU8, Ordering};

//     pub fn serialize<S>(atomic: &AtomicU8, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         serializer.serialize_u8(atomic.load(Ordering::Relaxed))
//     }

//     pub fn deserialize<'de, D>(deserializer: D) -> Result<AtomicU8, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let value = u8::deserialize(deserializer)?;
//         Ok(AtomicU8::new(value))
//     }
// }

// mod atomic_bool_serde {
//     use serde::{Deserialize, Deserializer, Serializer};
//     use std::sync::atomic::{AtomicBool, Ordering};

//     pub fn serialize<S>(atomic: &AtomicBool, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         serializer.serialize_bool(atomic.load(Ordering::Relaxed))
//     }

//     pub fn deserialize<'de, D>(deserializer: D) -> Result<AtomicBool, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let value = bool::deserialize(deserializer)?;
//         Ok(AtomicBool::new(value))
//     }
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyOptions {
    #[serde(with = "atomic_u8_serde")]
    pub low_battery: AtomicU8,

    #[serde(with = "atomic_bool_serde")]
    pub disconnection: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub reconnection: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub added: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub removed: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub stay_on_screen: AtomicBool,
}

#[test]
fn test() {
    let notify_options = NotifyOptions {
        low_battery: AtomicU8::new(15),
        disconnection: AtomicBool::new(false),
        reconnection: AtomicBool::new(false),
        added: AtomicBool::new(false),
        removed: AtomicBool::new(false),
        stay_on_screen: AtomicBool::new(false),
    };

    let toml_str = toml::to_string_pretty(&notify_options)
            .expect("Failed to serialize ConfigToml structure as a String of TOML.");

    std::fs::write(r"C:\Users\11593\Downloads\serde.toml", toml_str)
            .expect("Failed to TOML String to BlueGauge.toml");
}

macro_rules! impl_atomic_serde {
    ($mod_name:ident, $atomic_type:ty, $inner_type:ty) => {
        mod $mod_name {
            use serde::{Deserialize, Deserializer, Serializer};
            use std::sync::atomic::{Ordering, $atomic_type};

            pub fn serialize<S>(atomic: &$atomic_type, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_some(&atomic.load(Ordering::Relaxed))
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<$atomic_type, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = <$inner_type>::deserialize(deserializer)?;
                Ok(<$atomic_type>::new(value))
            }
        }
    };
}

impl_atomic_serde!(atomic_u8_serde, AtomicU8, u8);
impl_atomic_serde!(atomic_bool_serde, AtomicBool, bool);