use super::{Zeroize, ZeroizeOnDrop, Zeroizing};
use alloc::{boxed::Box, collections::BTreeMap};
use core::{borrow::Borrow, fmt};

/// A stable wipe-owned map key, exposed for borrowed-lookup bounds.
/// Its contents are available through the map's borrowed entry API.
#[doc(hidden)]
pub struct MapKey<K: Zeroize>(Box<Zeroizing<K>>);
impl<K: Zeroize> core::ops::Deref for MapKey<K> {
    type Target = K;
    fn deref(&self) -> &K {
        &self.0
    }
}
impl<K: Zeroize + PartialEq> PartialEq for MapKey<K> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl<K: Zeroize + Eq> Eq for MapKey<K> {}
impl<K: Zeroize + PartialOrd> PartialOrd for MapKey<K> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}
impl<K: Zeroize + Ord> Ord for MapKey<K> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (**self).cmp(&**other)
    }
}
impl<K: Zeroize> Borrow<K> for MapKey<K> {
    fn borrow(&self) -> &K {
        self
    }
}
impl<K: Zeroize + Borrow<str>> Borrow<str> for MapKey<K> {
    fn borrow(&self) -> &str {
        K::borrow(self)
    }
}
impl<K: Zeroize + Borrow<[u8]>> Borrow<[u8]> for MapKey<K> {
    fn borrow(&self) -> &[u8] {
        K::borrow(self)
    }
}

/// An ordered map with keys and values in stable wipe-owned allocations.
///
/// Tree balancing only moves pointers. Key order is deterministic and lookup
/// has logarithmic worst-case cost, including in no_std without random seeds.
pub struct Map<K: Zeroize, V: Zeroize>(BTreeMap<MapKey<K>, Box<Zeroizing<V>>>);
impl<K: Zeroize, V: Zeroize> Default for Map<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}
impl<K: Zeroize, V: Zeroize> Map<K, V> {
    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Borrow entries in ascending key order.
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.0.iter(),
        }
    }
    /// Borrow keys in ascending order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.0.keys().map(|key| &**key)
    }
    /// Borrow values in ascending key order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.0.values().map(|value| &***value)
    }
    /// Mutably borrow values in place, without moving their stable slots.
    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut V> + ExactSizeIterator {
        self.0.values_mut().map(|value| &mut ***value)
    }
    /// Wipe and remove every entry.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}
impl<K: Zeroize + Ord, V: Zeroize> Map<K, V> {
    /// Insert a value, wiping a replaced value and any redundant key before release.
    pub fn insert(&mut self, key: K, value: V) {
        self.0.insert(
            MapKey(Box::new(Zeroizing::new(key))),
            Box::new(Zeroizing::new(value)),
        );
    }
    /// Find a value by its key, a borrowed string, or borrowed bytes.
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        MapKey<K>: Borrow<Q>,
    {
        self.0.get(key).map(|value| &***value)
    }
    /// Mutably borrow an existing value without moving its storage.
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        MapKey<K>: Borrow<Q>,
    {
        self.0.get_mut(key).map(|value| &mut ***value)
    }
}
impl<K: Zeroize, V: Zeroize> Zeroize for Map<K, V> {
    fn zeroize(&mut self) {
        self.clear();
    }
}
impl<K: Zeroize, V: Zeroize> Drop for Map<K, V> {
    fn drop(&mut self) {
        self.zeroize();
    }
}
impl<K: Zeroize, V: Zeroize> ZeroizeOnDrop for Map<K, V> {}
impl<K: Zeroize + Ord + Clone, V: Zeroize + Clone> Clone for Map<K, V> {
    fn clone(&self) -> Self {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}
impl<K: Zeroize + Ord, V: Zeroize + PartialEq> PartialEq for Map<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}
impl<K: Zeroize + Ord, V: Zeroize + Eq> Eq for Map<K, V> {}
impl<K: Zeroize, V: Zeroize> fmt::Debug for Map<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Map([REDACTED])")
    }
}
impl<K: Zeroize + Ord, V: Zeroize> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::default();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}
impl<K: Zeroize + Ord, V: Zeroize> crate::map_codec::MapStorage for Map<K, V> {
    type Key = K;
    type Value = V;
    fn storage_len(&self) -> usize {
        self.len()
    }
    fn storage_insert(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
    fn storage_clear(&mut self) {
        self.clear();
    }
    fn storage_erase_key(key: &mut K) {
        key.zeroize();
    }
    fn storage_erase_value(value: &mut V) {
        value.zeroize();
    }
    fn storage_iter<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
    where
        K: 'a,
        V: 'a,
    {
        self.iter()
    }
}
/// Borrowed entries in ascending key order.
pub struct Iter<'a, K: Zeroize, V: Zeroize> {
    inner: alloc::collections::btree_map::Iter<'a, MapKey<K>, Box<Zeroizing<V>>>,
}
impl<'a, K: Zeroize, V: Zeroize> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(key, value)| (&**key, &***value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K: Zeroize, V: Zeroize> DoubleEndedIterator for Iter<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(key, value)| (&**key, &***value))
    }
}
impl<K: Zeroize, V: Zeroize> ExactSizeIterator for Iter<'_, K, V> {}
impl<K: Zeroize, V: Zeroize> core::iter::FusedIterator for Iter<'_, K, V> {}
impl<'a, K: Zeroize, V: Zeroize> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
#[cfg(feature = "json")]
impl<K: Zeroize + serde::Serialize, V: Zeroize + serde::Serialize> serde::Serialize for Map<K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
#[cfg(feature = "json")]
impl<'de, K: Zeroize + Ord + serde::Deserialize<'de>, V: Zeroize + serde::Deserialize<'de>>
    serde::Deserialize<'de> for Map<K, V>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<K, V>(core::marker::PhantomData<(K, V)>);
        impl<
                'de,
                K: Zeroize + Ord + serde::Deserialize<'de>,
                V: Zeroize + serde::Deserialize<'de>,
            > serde::de::Visitor<'de> for Visitor<K, V>
        {
            type Value = Map<K, V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut map = Map::default();
                while let Some(key) = access.next_key::<K>()? {
                    let key = Zeroizing::new(key);
                    let value = access.next_value()?;
                    map.0
                        .insert(MapKey(Box::new(key)), Box::new(Zeroizing::new(value)));
                }
                Ok(map)
            }
        }
        deserializer.deserialize_map(Visitor(core::marker::PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use core::sync::atomic::{AtomicUsize, Ordering};
    static ERASED: AtomicUsize = AtomicUsize::new(0);
    #[derive(Clone)]
    struct Counted(u64);
    impl Zeroize for Counted {
        fn zeroize(&mut self) {
            self.0.zeroize();
            ERASED.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn stable_values_are_wiped_on_replacement_clear_and_drop() {
        ERASED.store(0, Ordering::SeqCst);
        let mut map = Map::<String, Counted>::default();
        map.insert("key".into(), Counted(42));
        let address = map.get("key").unwrap() as *const Counted;
        for n in 0..100 {
            map.insert(alloc::format!("other-{n}"), Counted(n));
        }
        assert_eq!(address, map.get("key").unwrap() as *const Counted);
        map.insert("key".into(), Counted(43));
        assert_eq!(ERASED.load(Ordering::SeqCst), 1);
        map.get_mut("key").unwrap().0 = 44;
        map.clear();
        assert!(map.is_empty());
        assert_eq!(ERASED.load(Ordering::SeqCst), 102);
        map.insert("key".into(), Counted(45));
        drop(map);
        assert_eq!(ERASED.load(Ordering::SeqCst), 103);
    }
    #[cfg(feature = "json")]
    #[test]
    fn native_serde_handles_duplicates_and_invalid_values() {
        let map: Map<String, u64> = serde_json::from_str(r#"{"key":1,"key":2}"#).unwrap();
        assert_eq!(map.get("key"), Some(&2));
        assert_eq!(serde_json::to_string(&map).unwrap(), r#"{"key":2}"#);
        assert!(serde_json::from_str::<Map<String, u64>>(r#"{"key":1,"bad":[]}"#).is_err());
    }
}
