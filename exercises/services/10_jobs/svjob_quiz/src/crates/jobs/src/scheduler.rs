use crate::lease::LeaseStore;
use crate::periodic::Periodic;

pub fn leader_tick(periodic: &mut Periodic, leases: &mut LeaseStore, name: &str, owner: &str, now: u64, run: &mut impl FnMut()) -> bool {
    let _ = (periodic, leases, name, owner, now, run);
    todo!()
}
