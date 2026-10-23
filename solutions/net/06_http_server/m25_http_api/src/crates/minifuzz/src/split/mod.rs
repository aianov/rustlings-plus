use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitFailure<R> {
    pub cuts: Vec<usize>,
    pub whole: R,
    pub split: R,
}

pub fn check_splits<R: PartialEq + Debug>(
    input: &[u8],
    run: impl Fn(&[&[u8]]) -> R,
) -> Result<(), SplitFailure<R>> {
    let whole = run(&[input]);
    let mut attempts: Vec<Vec<usize>> = (1..input.len()).map(|cut| vec![cut]).collect();
    attempts.push((1..input.len()).collect());
    for cuts in attempts {
        let mut chunks = Vec::with_capacity(cuts.len() + 1);
        let mut previous = 0;
        for &cut in &cuts {
            chunks.push(&input[previous..cut]);
            previous = cut;
        }
        chunks.push(&input[previous..]);
        let split = run(&chunks);
        if split != whole {
            return Err(SplitFailure { cuts, whole, split });
        }
    }
    Ok(())
}
