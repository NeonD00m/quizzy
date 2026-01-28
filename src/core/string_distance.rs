use std::cmp::min;

/// some judgement of how far off the strings are
pub fn string_distance(given: String, expected: String) -> u8 {
    if given == expected {
        return 0;
    } else if given.is_empty() {
        return expected.len() as u8;
    } else if expected.is_empty() {
        println!("Expected string length should probably not be zero.");
        return given.len() as u8;
    }

    let s: Vec<char> = given.chars().collect();
    let t: Vec<char> = expected.chars().collect();

    let m = s.len();
    let n = t.len();

    if m > n {
        return string_distance(expected, given);
    }

    let mut v0: Vec<usize> = (0..=n).collect();
    let mut v1: Vec<usize> = vec![0usize; n + 1];

    // for i in 0..m {
    for (i, character) in s.iter().enumerate() {
        // if edit distance is deleting chars to match empty t
        v1[0] = i + 1;

        for j in 0..n {
            let deletion_cost = v0[j + 1].saturating_add(1);
            let insertion_cost = v1[j].saturating_add(1);
            let substitution_cost = v0[j].saturating_add(if *character == t[j] { 0 } else { 1 });
            v1[j + 1] = min(min(deletion_cost, insertion_cost), substitution_cost);
        }

        // omg holy improvement over what I was previously doing
        std::mem::swap(&mut v0, &mut v1);
    }

    // bro if string distance > 255 we don't care any more
    let dist_usize = v0[n];
    if dist_usize > u8::MAX as usize {
        u8::MAX
    } else {
        dist_usize as u8
    }
}
