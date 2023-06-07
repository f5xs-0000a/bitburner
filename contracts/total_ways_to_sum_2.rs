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
use std::collections::BTreeMap;
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

    dbg!(&known_solutions);

    eprintln!("Solutions: {}", known_solutions[&sum].len());
}

fn solve(
    sum: i64,
    values: &HashSet<i64>,
    known_solutions: &mut HashMap<i64, HashSet<BTreeMap<i64, usize>>>,
) {
    if let Some(solutions) = known_solutions.get(&sum) {
        return; // just ask the caller to find it inside known_solutions
    }

    if values.contains(&sum) {
        let mut sole_solution = BTreeMap::new();
        sole_solution.insert(sum, 1usize);

        let mut solution_set = HashSet::new();
        solution_set.insert(sole_solution);

        known_solutions.insert(sum, solution_set);
    }

    for value in values.iter().cloned() {
        let less = sum - value;

        if 0 < less {
            solve(less, values, known_solutions);

            let mut lesser_solutions = known_solutions.get(&less).unwrap().iter().cloned().map(|mut solution| { *solution.entry(value).or_insert(0) += 1; solution} ).collect::<HashSet<_>>();

            let mut target_solution = known_solutions.entry(sum).or_insert(HashSet::new());
            for solution in lesser_solutions.into_iter() {
                target_solution.insert(solution);
            }
        }
    }
}
