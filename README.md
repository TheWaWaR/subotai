# Subotai

 Subotai is a Kademlia based distributed hash table. It's designed to be easy to use, safe
 and quick. Here are some of the ideas that differentiate it from other DHTs:

 * **Externally synchronous, internally concurrent**: All public methods are blocking and return
   a sane result or an explicit timeout. Internally however, subotai is fully concurrent,
   and parallel operations will often help each other complete.

 * **Introduce nodes first, resolve conflicts later**: Subotai differs to the original Kademlia
   implementation in that it gives temporary priority to newer contacts for full buckets. This
   makes the network more dynamic and capable to adapt quickly, while still providing protection
   against basic `DDoS` attacks in the form of a defensive state.

 * **Flexible storage**: Every key in the key space can hold any number of different entries with
   independent expiration times. 

 * **Impatient**: Subotai is "impatient", in that it will attempt to never wait for responses from
 an unresponsive node. Queries are sent in parallel where possible, and processes continue when 
 a subset of nodes have responded.

 Subotai supports automatic key republishing, providing a good guarantee that an entry will remain
 in the network until a configurable expiration date. Manually storing the entry in the network
 again will refresh the expiration date.

 Subotai also supports caching to balance intense traffic around a given key.

[Docs](https://PabloMansanet.github.io/subotai/subotai)

# How to use

Let's say we have this code running on a machine:

```rust
fn main() {
   let node = node::Node::new().unwrap();

   // We join the network through the address of any live node.
   let seed = net::SocketAddr::from_str("192.168.1.100:50000").unwrap();
   node.bootstrap(&seed);

   // We store a key->data pair.
   let my_key = hash::SubotaiHash::sha1("example_key");
   let my_data = vec![0u8,1,2,3,4,5];
   node.store(my_key, node::StorageEntry::Blob(my_data));
}
```

As long as there is a minimum amount of nodes in the network (a configurable amount, deemed sufficient
for the network to be considered alive), we have successfully stored data in the table.

Now, on a machine very, very far away...

``` rust
fn main() {
   let node = node::Node::new().unwrap();

   // We join the same network (can be on a different node).
   let seed = net::SocketAddr::from_str("192.168.1.230:40000").unwrap();
   node.bootstrap(&seed);

   // We retrieve all entries for the same key.
   let my_key = hash::SubotaiHash::sha1("example_key");
   let retrieved_entries = node.retrieve(&my_key).unwrap();

   // We get what we stored. In O(log n) number of steps, of course!
   assert_eq!(retrieved_entries.len(), 1);
   let expected_data = vec![0u8,1,2,3,4,5];
   assert_eq!(*retrieved_entries.first().unwrap(), node::StorageEntry::Blob(expected_data));
}
```

# State of the Project

Subotai is feature complete, but in early stages of testing. I focused on making the API as stable 
as possible (partly by handling all configuration through a factory, to prevent new configuration options 
from becoming breaking changes) and providing a baseline of features for practical applications to be 
built on top of it. 

Even though every feature is unit tested, the network hasn't yet been tested in real world 
conditions. I'd really appreciate any experiments! Just make sure not to use it for anything critical
just yet :).

# Note on Testing

Subotai's unit test suite is somewhat fiddly, since the library uses timeouts pervasively in order to make
decisions. This means that, unless you're running a very powerful machine, you should run `cargo test` with
`RUST_TEST_THREADS=1`. Otherwise, an excessive number of tests in parallel may create timeouts all over the
place, causing the network to mistake nodes as dead.
