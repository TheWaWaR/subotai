use hash::HASH_SIZE;
use hash::Hash;
use std::net;
use std::collections::VecDeque;
use std::mem;
use std::sync::{Mutex, RwLock};

#[cfg(test)]
mod tests;

pub const ALPHA    : usize = 3;
pub const K        : usize = 20;
const BUCKET_DEPTH : usize = K;

/// Kademlia routing table, with 160 buckets of `BUCKET_DEPTH` (k) node
/// identifiers each, constructed around a parent node ID.
///
/// The structure employs least-recently seen eviction. Conflicts generated
/// by evicting a node by inserting a newer one remain tracked, so they can
/// be resolved later.
pub struct Table {
   buckets   : Vec<Bucket>,
   conflicts : Mutex<Vec<EvictionConflict>>,
   parent_id : Hash,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct NodeInfo {
   pub id      : Hash,
   pub address : net::SocketAddr,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum LookupResult {
   Myself,
   Found(NodeInfo), 
   ClosestNodes(Vec<NodeInfo>),
   Nothing,
}

impl Table {
   /// Constructs a routing table based on a parent node id. Other nodes
   /// will be stored in this table based on their distance to the node id provided.
   pub fn new(parent_id: Hash) -> Table {
      Table { 
         buckets   : (0..HASH_SIZE).map(|_| Bucket::new()).collect(),
         conflicts : Mutex::new(Vec::new()),
         parent_id : parent_id,
      }
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   /// by kicking out the oldest node in case the bucket is full, and registering
   /// an eviction conflict that can be revised later.
   pub fn insert_node(&self, info: NodeInfo) {
      if let Some(index) = self.bucket_for_node(&info.id) {
         let bucket = &self.buckets[index];
         let mut entries = bucket.entries.write().unwrap();

         entries.retain(|ref stored_info| info.id != stored_info.id);
         if entries.len() == BUCKET_DEPTH {
            let conflict = EvictionConflict { 
               evicted  : entries.pop_front().unwrap(),
               inserted : info.clone() 
            };
            let mut conflicts = self.conflicts.lock().unwrap();
            conflicts.push(conflict);
         }
         entries.push_back(info);
      }
   }

   /// Performs a node lookup on the routing table. The lookup result may
   /// contain the specific node, a list of up to the N closest nodes, or
   /// report that the parent node itself was requested.
   ///
   /// This employs an algorithm I have named "bounce lookup", which obtains
   /// the closest nodes to a given origin walking through the minimum 
   /// amount of buckets. It may exist already, but I haven't 
   /// found it any other implementation. It consists of:
   ///
   /// * Calculating the XOR distance between the parent node ID and the 
   ///   lookup node ID.
   ///
   /// * Checking the buckets indexed by the position of every "1" in said
   ///   distance hash, in descending order.
   ///
   /// * "Bounce" back up, checking the buckets indexed by the position of
   ///   every "0" in that distance hash, in ascending order.
   pub fn lookup(&self, id: &Hash, n: usize, blacklist: Option<&Vec<Hash>>) -> LookupResult {
      if id == &self.parent_id {
         return LookupResult::Myself;
      }

      match self.specific_node(id) {
         Some(info) => LookupResult::Found(info),
         None =>  {
            let closest = self.closest_n_nodes_to(id, n, blacklist);
            if closest.is_empty() {
               LookupResult::Nothing
            } else {
               LookupResult::ClosestNodes(closest)
            }
         }
      }
   }

   /// Returns an iterator over all stored nodes, ordered by ascending
   /// distance to the parent node. This iterator is designed for concurrent
   /// access to the data structure, and as such it isn't guaranteed that it
   /// will return a "snapshot" of all nodes for a specific moment in time. 
   /// Buckets already visited may be modified elsewhere through iteraton, 
   /// and unvisited buckets may accrue new nodes.
   pub fn all_nodes(&self) -> AllNodes {
      AllNodes {
         table          : self,
         current_bucket : Vec::with_capacity(BUCKET_DEPTH),
         bucket_index   : 0,
      }
   }

   /// Returns a table entry for the specific node with a given hash.
   pub fn specific_node(&self, id: &Hash) -> Option<NodeInfo> {
      if let Some(index) = self.bucket_for_node(id) {
         let entries = &self.buckets[index].entries.read().unwrap();
         return entries.iter().find(|ref info| *id == info.id).cloned();
      }
      None
   }

   /// Bounce lookup algorithm.
   fn closest_n_nodes_to(&self, id: &Hash, n: usize, blacklist: Option<&Vec<Hash>>) -> Vec<NodeInfo> {
      let mut closest = Vec::with_capacity(n);
      let distance = &self.parent_id ^ id;
      let descent  = distance.ones().rev();
      let ascent   = distance.zeroes();
      let lookup_order = descent.chain(ascent);
      
      for bucket_index in lookup_order {
         let entries = self.buckets[bucket_index].entries.read().unwrap();
         if entries.is_empty() {
            continue;
         }
         
         let mut nodes_from_bucket = entries.clone().into_iter().collect::<Vec<NodeInfo>>();
         if let Some(blacklist) = blacklist {
            nodes_from_bucket.retain(|node: &NodeInfo| !blacklist.contains(&node.id));
         }

         nodes_from_bucket.sort_by_key(|ref info| &info.id ^ id);
         let space_left = closest.capacity() - closest.len();
         nodes_from_bucket.truncate(space_left);
         closest.append(&mut nodes_from_bucket);

         if closest.len() == closest.capacity() {
            break;
         }
      }
      closest
   }

   /// Returns the appropriate position for a node, by computing
   /// the index where their prefix starts differing. If we are requesting
   /// the bucket for this table's own parent node, it can't be stored.
   fn bucket_for_node(&self, id: &Hash) -> Option<usize> {
       (&self.parent_id ^ id).height()
   }

   fn revert_conflict(&self, conflict: EvictionConflict) {
      if let Some(index) = self.bucket_for_node(&conflict.inserted.id) {
         let mut entries = self.buckets[index].entries.write().unwrap();
         let evictor = &mut entries.iter_mut().find(|ref info| conflict.inserted.id == info.id).unwrap();
         mem::replace::<NodeInfo>(evictor, conflict.evicted);
      }
   }
}

/// Produces copies of all known nodes, ordered in ascending
/// distance from self. It's a weak invariant, i.e. the table
/// may be modified through iteration.
pub struct AllNodes<'a> {
   table          : &'a Table,
   current_bucket : Vec<NodeInfo>,
   bucket_index   : usize,
}

/// Represents a conflict derived from attempting to insert a node in a full
/// bucket. 
#[derive(Debug,Clone)]
struct EvictionConflict {
   evicted  : NodeInfo,
   inserted : NodeInfo
}

/// Bucket size is estimated to be small enough not to warrant
/// the downsides of using a linked list.
///
/// Each vector of bucket entries is protected under its own mutex, to guarantee 
/// concurrent access to the table.
#[derive(Debug)]
struct Bucket {
   entries: RwLock<VecDeque<NodeInfo>>,
}

impl<'a> Iterator for AllNodes<'a> {
   type Item = NodeInfo;

   fn next(&mut self) -> Option<NodeInfo> {
      while self.bucket_index < HASH_SIZE && self.current_bucket.is_empty() {
         let mut new_bucket = { // Lock scope
            self.table.buckets[self.bucket_index].entries.read().unwrap().clone()
         }.into_iter().collect::<Vec<NodeInfo>>();

         new_bucket.sort_by_key(|ref info| &info.id ^ &self.table.parent_id);
         self.current_bucket.append(&mut new_bucket);
         self.bucket_index += 1;
      }
      self.current_bucket.pop()
   } 
}

impl Bucket {
   fn new() -> Bucket {
      Bucket{
         entries: RwLock::new(VecDeque::with_capacity(BUCKET_DEPTH))
      }
   }
}
