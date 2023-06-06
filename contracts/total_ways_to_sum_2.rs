// # Total Ways to Sum II
// 
// You are attempting to solve a Coding Contract. You have 10 tries remaining,
// after which the contract will self-destruct.
// 
// How many different distinct ways can the number [input] be written as a sum
// of integers contained in the set:
// 
// [input]?
// 
// You may use each integer in the set zero or more times.

use std::io::BufRead as _;
use std::collections::HashMap;
use std::collections::HashSet;

fn get_a_number(prompt: &str) -> i64 {
    let mut stdin_lock = std::io::stdin().lock();

    eprint!("{}", prompt);
    let mut buffer = String::new();
    loop {
        match stdin_lock.read_line(&mut buffer) {
            Ok(x) if x != 0 => {
                buffer.pop();
                if let Ok(val) = buffer.parse() {
                    return val;
                }

            }
            _ => {},
        }

        eprintln!("Invalid value.");
    }
}

fn main() {
    let sum = get_a_number("Enter sum: ");

    let mut values = HashSet::new();
    loop {
        let val = get_a_number("Enter a number of the set (enter 0 to solve now): ");
        if val == 0 {
            break;
        }

        values.insert(val);
    }

    let mut known_solutions = HashMap::new();
    let solutions = solve(sum, &values, &mut known_solutions);

    eprintln!("Solutions: {}", solutions);
}

fn solve(
    sum: i64,
    values: &HashSet<i64>,
    known_solutions: &mut HashMap<i64, usize>
) -> usize {
    if let Some(solutions) = known_solutions.get(&sum) {
        return *solutions;
    }

    let mut solutions = 0;
    for value in values.iter().cloned() {
        let less = sum - value;

        if less == 0 {
            solutions += 1;
        }

        else if 0 < less {
            solutions += solve(less, values, known_solutions);
        }
    }

    known_solutions.insert(sum, solutions);
    solutions
}
