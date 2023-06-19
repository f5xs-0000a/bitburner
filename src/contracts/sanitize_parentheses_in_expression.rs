//! Sanitize Parentheses in Expression
//!
//! Given an input string, remove the minimum number of invalid parentheses in
//! order to validate the string. If there are multiple minimal ways to validate
//! the string, provide all of the possible results. The answer should be
//! provided as an array of strings. If it is impossible to validate the string
//! the result should be an array with only an empty string.

fn solve(input: &str) -> Vec<String> {
    // an interesting way to rephrase this is considering that opening braces
    // are +1 and closing braces are -1. for each of the parentheses, add their
    // corresponding values into the sum. plot the sum along with the
    // parentheses. for example:
    //
    // chars  (  (  (  )  )  )
    // sums   1  2  3  2  1  0
    //
    // in order for a parentheses string to be valid:
    // 1. the sum must end in 0, and
    // 2. the sum never goes negative
    //
    // chars  (  (  (  )  )
    // sums   1  2  3  2  1  (invalid due to not ending with 0)
    //
    // chars  (  )  )  (  )
    // sums   1  0 -1  0 -1  (invalid due to the sum dipping to negatives)
    //
    // in case the current string is invalid, there is only one way to check if
    // there are characters removable that can make the string valid.
    // 1. there must be a sum anywhere that is higher than the final sum
    // for all the sections that have dipped to negatives, it is considered
    // invalid
    //
    // chars  (  )  )  (  )
    // sums   1  0 -1  0 -1
    //       (invalid)

    let (skip_amount, sums) = loop {
        let mut skip_amount = 0;

        // plot the sums
        let sums = input.chars()
            .map(|c| match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            })
            .skip(skip_amount)
            .scan(0, |mut state, x| {
                *state += x;
                Some(*state)
            })
            .collect::<Vec<_>>();

        let last_sum = sums.last().unwrap_or(0);

        if sums.last().unwrap_or(0) < 0 {
            // on the case that the last sum is negative, find on the sums the
            // earlist value that is the same as the last sum
            match sums.iter().find(|s| *s == s) {
                // the sum isn't found. there is none.
                None => return vec![],
                Some(idx) => {
                    skip_amount = idx + 1,
                    continue;
                },
            };
        }

        break (skip_amount, sums);
    };

    // if the sum is valid in the first place by having no negatives, return it
    if sums.iter().all(|x| 0 <= x) {
        return vec![input.to_owned()];
    }

    // count the number of excess opens and closes
    let excess_opens = sums.last().unwrap().max(0);
    let excess_closes = (-sums.last().unwrap()).max(0);
}

#[cfg(test)]
mod test {
    #[test]
    fn solve_that() {
        super::solve(")(");
        panic!();
    }
}
