use crate::Db;
use events::Journal;

pub fn relay_once(db: &Db, journal: &Journal, batch: usize) -> Result<usize, String> {
    let mut sent = 0;
    for row in db.pending(batch) {
        journal.append(&row.event)?;
        db.mark_sent(row.seq).map_err(|e| format!("{e:?}"))?;
        sent += 1;
    }
    Ok(sent)
}
