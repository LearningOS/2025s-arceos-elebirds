extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

const FIBONACCI_MAGIC: u64 = 1_140_071_481_932_319_848;

#[derive(Clone, Copy, Default, Hash)]
pub struct FibonacciHash(u64);

impl Hasher for FibonacciHash {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = FIBONACCI_MAGIC.wrapping_mul(self.0).wrapping_add(*byte as u64);
        }
    }
}

#[derive(Hash)]
pub struct HashMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone
{
    entries: Vec<Vec<HashMapEntry<K, V>>>,
    size: usize,
    capacity: usize,
    capacity_mask: usize,
    hasher: FibonacciHash,
}

impl<K, V> HashMap<K, V> 
where
    K: Eq + Hash + Clone,
    V: Clone
{
    pub fn new() -> Self {
        Self {
            entries: vec![Vec::new(); 16],
            size: 0,
            capacity: 16,
            capacity_mask: 15,
            hasher: FibonacciHash::default(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            entries: vec![Vec::new(); capacity],
            size: 0,
            capacity,
            capacity_mask: capacity - 1,
            hasher: FibonacciHash::default(),
        }
    }

    pub fn hash(&self, key: &K) -> usize {
        let mut hasher = self.hasher.clone();
        key.hash(&mut hasher);
        hasher.finish() as usize & self.capacity_mask
    }

    pub fn insert(&mut self, key: K, value: V) {
        let index = self.hash(&key);
        let entry = HashMapEntry::new(key, value);
        self.entries[index].push(entry);
        self.size += 1;
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let index = self.hash(key);
        self.entries[index].iter().find(|entry| entry.key == *key).map(|entry| &entry.value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = self.hash(key);
        self.entries[index].iter().position(|entry| entry.key == *key).map(|i| {
            self.size -= 1;
            self.entries[index].remove(i).value
        })
    }

    pub fn clear(&mut self) {
        self.entries.iter_mut().for_each(|entry| entry.clear());
        self.size = 0;
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().flat_map(|entry| entry.iter()).map(|entry| (&entry.key, &entry.value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct HashMapEntry<K, V> {
    key: K,
    value: V,
}

impl<K, V> HashMapEntry<K, V> {
    fn new(key: K, value: V) -> Self {
        Self { key, value }
    }
}

