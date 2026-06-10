use std::sync::atomic::{AtomicU64, Ordering};

const PROBE_LIMIT: u32 = 4;

#[derive(Default)]
struct Slot {
    key: String,
    value: String,
    key_hash: u64,
    ref_bit: bool,
    present: bool,
}

pub struct ContentCache {
    slots: Vec<Slot>,
    capacity: u32,
    hand: u32,
    count: u32,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub count: u32,
    pub capacity: u32,
}

impl ContentCache {
    pub fn new(capacity: u32) -> Self {
        assert!(capacity >= 1, "ContentCache capacity must be >= 1");
        let mut slots = Vec::with_capacity(capacity as usize);
        slots.resize_with(capacity as usize, Slot::default);
        Self {
            slots,
            capacity,
            hand: 0,
            count: 0,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    fn hash_key(key: &str) -> u64 {
        let mut h: u64 = 14695981039346656037;
        for b in key.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        if h == 0 {
            1
        } else {
            h
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        let h = Self::hash_key(key);
        let base = (h as u32) % self.capacity;
        for i in 0..PROBE_LIMIT {
            let slot_idx = (base.wrapping_add(i)) % self.capacity;
            let slot = &self.slots[slot_idx as usize];
            if slot.present && slot.key_hash == h && slot.key == key {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(&slot.value);
            }
            if !slot.present && slot.key_hash == 0 {
                break;
            }
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    pub fn put(&mut self, key: String, value: String) {
        let h = Self::hash_key(&key);
        let base = (h as u32) % self.capacity;
        let mut tombstone_idx = None;

        for i in 0..PROBE_LIMIT {
            let slot_idx = (base.wrapping_add(i)) % self.capacity;
            let slot = &mut self.slots[slot_idx as usize];
            if slot.present && slot.key_hash == h && slot.key == key {
                slot.value = value;
                slot.ref_bit = true;
                return;
            }
            if !slot.present && slot.key_hash != 0 && tombstone_idx.is_none() {
                tombstone_idx = Some(slot_idx);
            }
            if !slot.present && slot.key_hash == 0 {
                let insert_idx = tombstone_idx.unwrap_or(slot_idx);
                self.insert_at(insert_idx, key, value, h);
                return;
            }
        }

        let insert_idx = tombstone_idx.unwrap_or_else(|| self.clock_evict());
        self.insert_at(insert_idx, key, value, h);
    }

    pub fn remove(&mut self, key: &str) {
        let h = Self::hash_key(key);
        let base = (h as u32) % self.capacity;
        for i in 0..PROBE_LIMIT {
            let slot_idx = (base.wrapping_add(i)) % self.capacity;
            let slot = &mut self.slots[slot_idx as usize];
            if slot.present && slot.key_hash == h && slot.key == key {
                slot.present = false;
                slot.key.clear();
                slot.value.clear();
                slot.ref_bit = false;
                self.count -= 1;
                return;
            }
            if !slot.present && slot.key_hash == 0 {
                return;
            }
        }
    }

    fn insert_at(&mut self, slot_idx: u32, key: String, value: String, key_hash: u64) {
        let slot = &mut self.slots[slot_idx as usize];
        if slot.present {
            self.count -= 1;
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
        slot.key = key;
        slot.value = value;
        slot.key_hash = key_hash;
        slot.ref_bit = true;
        slot.present = true;
        self.count += 1;
    }

    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.present = false;
            slot.key_hash = 0;
            slot.ref_bit = false;
            slot.key.clear();
            slot.value.clear();
        }
        self.count = 0;
        self.hand = 0;
    }

    #[cfg(test)]
    pub fn len(&self) -> u32 {
        self.count
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[cfg(test)]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            count: self.count,
            capacity: self.capacity,
        }
    }

    fn clock_evict(&mut self) -> u32 {
        let cap = self.capacity;
        let mut sweeps = 0u32;
        while sweeps < cap * 2 {
            let slot_idx = self.hand % cap;
            self.hand = (self.hand.wrapping_add(1)) % cap;
            let slot = &self.slots[slot_idx as usize];
            if !slot.present {
                return slot_idx;
            }
            if !slot.ref_bit {
                return slot_idx;
            }
            self.slots[slot_idx as usize].ref_bit = false;
            sweeps += 1;
        }
        let slot_idx = self.hand % cap;
        self.hand = (self.hand.wrapping_add(1)) % cap;
        slot_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_get_put() {
        let mut cache = ContentCache::new(64);
        cache.put("foo".to_string(), "bar".to_string());
        assert_eq!(cache.get("foo"), Some("bar"));
        assert!(cache.get("missing").is_none());
    }

    #[test]
    fn put_updates_existing() {
        let mut cache = ContentCache::new(64);
        cache.put("key".to_string(), "v1".to_string());
        cache.put("key".to_string(), "v2".to_string());
        assert_eq!(cache.get("key"), Some("v2"));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn clear_drops_all() {
        let mut cache = ContentCache::new(64);
        cache.put("a".to_string(), "1".to_string());
        cache.put("b".to_string(), "2".to_string());
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn eviction_under_pressure() {
        let mut cache = ContentCache::new(50);
        for i in 0..100 {
            cache.put(format!("file_{i}.zig"), format!("content_{i}"));
        }
        assert!(cache.len() <= 50);
        let stats = cache.stats();
        assert!(stats.evictions > 0);
        assert_eq!(stats.count, cache.len());
        assert_eq!(stats.capacity, 50);
        assert!(stats.hits + stats.misses <= 100);
    }

    #[test]
    fn remove_preserves_collided_probe_chain() {
        let capacity = 8;
        let mut keys = Vec::new();
        for i in 0..1000 {
            let key = format!("key_{i}");
            if (ContentCache::hash_key(&key) as u32).is_multiple_of(capacity) {
                keys.push(key);
                if keys.len() == 2 {
                    break;
                }
            }
        }

        let mut cache = ContentCache::new(capacity);
        cache.put(keys[0].clone(), "first".to_string());
        cache.put(keys[1].clone(), "second".to_string());
        cache.remove(&keys[0]);

        assert_eq!(cache.get(&keys[1]), Some("second"));
    }
}
