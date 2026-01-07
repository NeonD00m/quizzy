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

    let m = given.len();
    let n = expected.len();

    if m > n {
        return string_distance(expected, given);
    }
    let mut v0: Vec<u8> = Vec::with_capacity(n + 1);
    let mut v1: Vec<u8> = Vec::with_capacity(n + 1);
    let s: Vec<char> = given.chars().collect();
    let t: Vec<char> = given.chars().collect();

    {
        let mut i: u8 = 0;
        v0.fill_with(|| {
            i += 1;
            return i - 1;
        });
        v1.fill(0); // make sure vector is not 'empty' even though allocated
    }

    for i in 0..(m - 1) {
        // if edit distance is deleting chars to match empty t
        v1[0] = i as u8 + 1;

        for j in 0..(n - 1) {
            let deletion_cost = v0[j + 1] + 1;
            let insertion_cost = v1[j] + 1;
            let substitution_cost = v0[j] + (if s[i] == t[j] { 0 } else { 1 });
            v1[j + 1] = min(min(deletion_cost, insertion_cost), substitution_cost);
        }

        v0 = v1;
        v1 = Vec::with_capacity(n + 1);
    }

    return v0[n];
}
