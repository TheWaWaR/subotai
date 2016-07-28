//! #Receptions
//!
//! A receptions object is a convenient iterator over all RPCs received
//! by a node. 
//!
//! By default, iterating over a Receptions object will block indefinitely
//! while waiting for packet arrivals, but it's possible to specify an
//! imprecise timeout so the iterator is only valid for a span of time.
//!
//! It is also possible to filter the iterator so it only applies to particular
//! senders or RPC kinds without resorting to iterator adapters.

use {bus, rpc, time};
use node::resources;
use hash::Hash;

/// A blocking iterator over the RPCs received by a node.
pub struct Receptions {
   iter          : bus::BusIntoIter<resources::Update>,
   timeout       : Option<time::SteadyTime>,
   kind_filter   : Option<KindFilter>,
   sender_filter : Option<Vec<Hash>>,
   shutdown      : bool,
}

#[derive(Eq, PartialEq, Debug)]
pub enum KindFilter {
   Ping,
   PingResponse,
   Store,
   FindNode,
   FindNodeResponse,
   FindValue,
   FindValueResponse,
   Bootstrap,
   BootstrapResponse,
}

impl Receptions {
   pub fn new(resources: &resources::Resources) -> Receptions {
      Receptions {
         iter          : resources.updates.lock().unwrap().add_rx().into_iter(),
         timeout       : None,
         kind_filter    : None,
         sender_filter : None,
         shutdown      : false,
      }
   }

   /// Restricts the iterator to a particular span of time.
   pub fn during(mut self, lifespan: time::Duration) -> Receptions {
      self.timeout = Some(time::SteadyTime::now() + lifespan);
      self
   }

   /// Only produces a particular rpc kind.
   pub fn of_kind(mut self, filter: KindFilter) -> Receptions {
      self.kind_filter = Some(filter);
      self
   }

   /// Only from a sender.
   pub fn from(mut self, sender: Hash) -> Receptions {
      self.sender_filter = Some(vec![sender]);
      self
   }

   /// Only from a set of senders.
   pub fn from_senders(mut self, senders: Vec<Hash>) -> Receptions {
      self.sender_filter = Some(senders);
      self
   }
}

impl Iterator for Receptions {
   type Item = rpc::Rpc;

   fn next(&mut self) -> Option<rpc::Rpc> {
      loop {
         if let Some(timeout) = self.timeout {
            if time::SteadyTime::now() > timeout {
               break;
            }
         }
         if self.shutdown {
            break;
         }

         //if let Some(resources::Update::RpcReceived(rpc)) = self.iter.next() {
         match self.iter.next() {
            Some(resources::Update::RpcReceived(rpc)) => {
               if let Some(ref kind_filter) = self.kind_filter {
                  match rpc.kind {
                     rpc::Kind::Ping                 => if *kind_filter != KindFilter::Ping { continue; },
                     rpc::Kind::PingResponse         => if *kind_filter != KindFilter::PingResponse { continue; },
                     rpc::Kind::Store(_)             => if *kind_filter != KindFilter::Store { continue; },
                     rpc::Kind::FindNode(_)          => if *kind_filter != KindFilter::FindNode { continue; },
                     rpc::Kind::FindNodeResponse(_)  => if *kind_filter != KindFilter::FindNodeResponse { continue; },
                     rpc::Kind::FindValue(_)         => if *kind_filter != KindFilter::FindValue { continue; },
                     rpc::Kind::FindValueResponse(_) => if *kind_filter != KindFilter::FindValueResponse { continue; },
                     rpc::Kind::Bootstrap            => if *kind_filter != KindFilter::Bootstrap { continue; },
                     rpc::Kind::BootstrapResponse(_) => if *kind_filter != KindFilter::BootstrapResponse { continue; },
                  }
               }

               if let Some(ref sender_filter) = self.sender_filter {
                  if !sender_filter.contains(&rpc.sender_id) {
                     continue;
                  }
               }

               return Some(rpc);
            },
            Some(resources::Update::Shutdown) => self.shutdown = true,
            _ => (),
         }
      }
      None
   }
}

#[cfg(test)]
mod tests {
    use node;
    use time;
    use super::KindFilter;

    #[test]
    fn produces_rpcs_but_not_ticks() {
       let alpha = node::Node::new().unwrap();
       let beta = node::Node::new().unwrap();
       let table_size = alpha.bootstrap_until(beta.local_info(), 1).unwrap();

       assert_eq!(table_size, 1);
       let beta_receptions = 
          beta.receptions()
              .during(time::Duration::seconds(1))
              .rpc(KindFilter::Ping);

       assert!(alpha.ping(beta.local_info().id).is_ok());
       assert!(alpha.ping(beta.local_info().id).is_ok());

       assert_eq!(beta_receptions.count(),2);
    }

    #[test]
    fn sender_filtering() {
       let receiver = node::Node::new().unwrap();
       let alpha = node::Node::new().unwrap();
       let beta  = node::Node::new().unwrap();
       
       let mut allowed = Vec::new();
       allowed.push(beta.local_info().id);
      
       let receptions = 
          receiver.receptions()
                  .during(time::Duration::seconds(1))
                  .from_senders(allowed)
                  .rpc(KindFilter::Ping);

       assert!(alpha.bootstrap_until(receiver.local_info(), 1).is_ok());
       assert!(beta.bootstrap_until(receiver.local_info(), 1).is_ok());

       assert!(alpha.ping(receiver.local_info().id).is_ok());
       assert!(beta.ping(receiver.local_info().id).is_ok());

       assert_eq!(receptions.count(),1);
    }
}


