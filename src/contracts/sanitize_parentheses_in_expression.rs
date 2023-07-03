//! Sanitize Parentheses in Expression
//!
//! Given an input string, remove the minimum number of invalid parentheses in
//! order to validate the string. If there are multiple minimal ways to validate
//! the string, provide all of the possible results. The answer should be
//! provided as an array of strings. If it is impossible to validate the string
//! the result should be an array with only an empty string.

fn solve(input: &str) -> Vec<String> {
    // one way to solve this is by using stacks and iterators. for every
    // character on the string, pair it with its index and prepare to push it
    // into the stack.
    // stack.
    // 
    // if it is not a parenthesis, immediately pop it out of the stack.
    // if it is an opening parenthesis, push it into the stack.
    // if it is a closing parenthesis, pop from the stack.
    //     if it is an opening parenthesis, keep both parentheses out of the
    //         stack
    //     if it is not, record the first index this has happened. also record
    //         how many times this has happened
    //
    // for the remaining opening parentheses still in the stack, record the
    // first index it has happened and how many are remaining
    //
    // for both cases, you will have the index of when it happened first/last
    // and its occurrences.
    // 
    // if a closing parenthes
    //
    // note: doesn't work in )()())(

    let mut cur_vec = Vec::with_capacity(input.len());

    let mut sum = 0;
    for (idx, ch) in input.chars().enumerate() {
        let open = match ch {
            '(' => true,
            ')' => false,
            _ => continue,
        };

        if open {
            sum += 1;
        }

        else {
            sum -= 1;
        }

        char_vec.push((open, idx, sum));
    }

    let mut stack_counter = 0usize;
    let mut over_close = 0;
    let mut lowest_sum = 0;
    let mut latest_close_index = 0;
    let mut earliest_close_index = 0;

    for (open, idx, sum) in cur_vec.iter() {
        if open {
            stack += 1;
        }

        else if 0 < stack_counter {
            stack_counter -= 1
        }

        else {
            over_close += 1;
            latest_close_index = idx;
        }

        if sum < lowest_sum {
            lowest_sum = sum;
            earliest_close_index = idx;
        }
    }

    let over_open = sum - lowest_sum;

    // we've got the points of until when we're going to remove parentheses and
    // how many. which groups of parentheses and how many of them are we going
    // to remove per group?
    //
    // that's obtained by checking how many pairs already existed for a given
    // smth


}

#[cfg(test)]
mod test {
    #[test]
    fn solve_that() {
        super::solve(")(");
        panic!();
    }
}
