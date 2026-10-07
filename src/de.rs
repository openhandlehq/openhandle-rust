use serde::{Deserialize, Deserializer};

/// Reads `null` as the default value, so a field the contract marks as
/// required never fails a whole response when the API leaves it empty.
pub(crate) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}
