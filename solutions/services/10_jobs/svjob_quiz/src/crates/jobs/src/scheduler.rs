use crate::lease::LeaseStore;
use crate::periodic::Periodic;

pub fn leader_tick(periodic: &mut Periodic, leases: &mut LeaseStore, name: &str, owner: &str, now: u64, run: &mut impl FnMut()) -> bool {
    if periodic.due(now).is_empty() {
        return false;
    }
    let slot = now / periodic.every();
    if leases.acquire(&format!("{name}:{slot}"), owner, now).is_none() {
        return false;
    }
    run();
    true
}
