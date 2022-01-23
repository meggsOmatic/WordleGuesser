mod scrabble_word_list;
mod word_frequency_list;
mod wordle_solutions;
use clap::Parser;
use itertools::Itertools;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::*;
use std::io;
use std::io::prelude::*;

const WORD_LENGTH: usize = 5;

// We can represent the "score" of a guess versus a target as a single number. There are
// five letters and three possibilities for each letter, for a total of 3^5 possible
// ways of scoring a guess against a target. We can think of this as a 5-digit base-three
// number, where:
//
//     0 = letter not in target
//     1 = right letter in wrong place (yellow)
//     2 = right letter in right place (green)
//
// So if we're guessing "caddy" against a secret word of "abbey", the score would be:
//
//    C: 0 = not in solution
//    A: 1 = wrong place (yellow)
//    D: 0 = not in solution
//    D: 0 = not in solution
//    Y: 2 = right place (green)
//
// And reading bottom up, that can be represented as a base-3 number of 20010 (for convenience the
// first letter is in the lowest digit). Which would turn into a decimal number as:
//
//   162 +     [162 == 2 * 3^4 == 2 * 81]
//     0 +     [  0 == 0 * 3^3 == 0 * 27]
//     0 +     [  0 == 0 * 3^2 == 0 *  9]
//     3 +     [  3 == 1 * 3^1 == 1 *  3]
//     0       [  0 == 0 * 3^0 == 0 *  1]
// --------
//   165
//
// And so the score for guessing "caddy" against a possible secret word of "abbey" would be 165.
//
// Higher-numbered scores aren't better in any way -- the point is we can make a histogram of
// the possible scores for a single guess against a list of possible solutions, and then
// rank the guesses based on which one does the best at narrowing the list of possible solutions.

const NUM_SCORES: usize = 243; // This is pow(3, WORD_LENGTH). Any good way to make that compile-time?
type WordScore = u8;

// Readable scores are in a format like ".y.GG", where:
//   . = letter not found
//   y = (yellow) letter in wrong place
//   G = (green) letter in right place

// Turn a numeric score into something readable. 165 => .y..G
fn format_score(mut score: WordScore) -> String {
    let mut result = String::with_capacity(WORD_LENGTH);
    for _ in 0..WORD_LENGTH {
        let letter_score = score % 3;
        result.push(match letter_score {
            0 => '.',
            1 => 'y',
            2 => 'G',
            _ => panic!("{} is not in 0..2", letter_score),
        });
        score = score / 3;
    }

    result
}

// Try to turn a readable string back into a numeric score. .y..G => 165
fn parse_score(readable: &str) -> Option<WordScore> {
    if readable.len() != WORD_LENGTH {
        return None;
    }

    let mut result = 0;
    let mut mult = 1;
    for &c in readable.as_bytes() {
        result += match c as char {
            'g' | 'G' => 2,
            'y' | 'Y' => 1,
            '.' => 0,
            _ => {
                return None;
            }
        } * mult;
        mult *= 3;
    }
    Some(result)
}

// Calculate the score for a given guess against a given target. Note that this is NOT symmetric.
// i.e.  score_word_pair("caddy", "abbey") != score_word_pair("abbey", "caddy")
//
// Because this function consumes the majority of the runtime, it's been superseded by the
// hand-optimized version below. Kept around for reference and to validate the correctness
// of the optimized version.
#[allow(dead_code)]
fn score_word_pair_simple(guess: &str, target: &str) -> WordScore {
    // A bitfield for the letters of the guess and the target. We
    // mark these off as they're paired up.
    let mut guess_used = 0u32;
    let mut target_used = 0u32;

    // The result. Starts at 0 for no matches; as we find matches
    // we'll add values in.
    let mut result: WordScore = 0;

    // Reasonable to use bytes here. We're playing a game about
    // guessing English words, and the speed of this function is
    // the main limit in performance.
    let guess = guess.as_bytes();
    let target = target.as_bytes();

    // Match up all of the "right letter in right place" pairs FIRST,
    // and mark them off as so they won't be checked later. If we're
    // matching "cheer" against "abbey" we want to have the SECOND 'e'
    // in "cheEr" be scored as a right-letter-right-place match, and
    // do NOT want the FIRST 'e' to be scored as a right-letter-wrong-place
    // match.
    //
    // When we find a match, add a 2 in the corresponding place in
    // the score.
    let mut mult: WordScore = 1;
    for i in 0..WORD_LENGTH {
        if guess[i] == target[i] {
            result += 2 * mult;
            guess_used |= 1 << i;
            target_used |= 1 << i;
        }
        mult *= 3;
    }

    // Now match the remaining letters, searching for other places.
    // Here we have to consider all 5*5 pairings. Getting clever
    // about skipping past things in the iteration is unlikely to
    // be faster than a simple constant-size loop.
    //
    // When we find a match, add a 1 in the corresponding place in
    // the score.
    mult = 1;
    for i in 0..WORD_LENGTH {
        if (guess_used & (1 << i)) != 0 {
            mult *= 3;
            continue;
        }
        let g = guess[i];
        for j in 0..WORD_LENGTH {
            if (target_used & (1 << j)) != 0 {
                continue;
            }
            if g == target[j] {
                guess_used |= 1 << i;
                target_used |= 1 << j;
                result += 1 * mult;
                break;
            }
        }
        mult *= 3;
    }

    result
}

// Calculate the score for a given guess against a given target. Note that this is NOT symmetric.
// i.e.  score_word_pair("caddy", "abbey") != score_word_pair("abbey", "caddy")
//
// This version is hand-unrolled and uses unsafe pointers, which combine to make it about 75%
// faster than the score_word_simple above. It should always generate the same output for
// the same inputs, though.
fn score_word_pair(guess: &str, target: &str) -> WordScore {
    if WORD_LENGTH != 5 {
        panic!(
            "WORD_LENGTH is {} but score_word_pair was hand-optimized for 5",
            WORD_LENGTH
        );
    }

    if guess.len() != WORD_LENGTH {
        panic!("guess '{}' is not exactly length {}", guess, WORD_LENGTH);
    }

    if target.len() != WORD_LENGTH {
        panic!("target '{}' is not exactly length {}", guess, WORD_LENGTH);
    }

    // The result. Starts at 0 for no matches; as we find matches
    // we'll add values in.
    let mut result: WordScore = 0;

    unsafe {
        // A bitfield for the letters of the guess and the target. We
        // mark these off as they're paired up.
        let mut guess_used = 0u32;

        // Reasonable to use bytes and pointers here. We're playing a game about
        // guessing English words, and the speed of this function is
        // the main limit in performance.
        let guess = guess.as_ptr();
        let target = target.as_ptr();

        // Match up all of the "right letter in right place" pairs FIRST,
        // and mark them off as so they won't be checked later. If we're
        // matching "cheer" against "abbey" we want to have the SECOND 'e'
        // in "cheEr" be scored as a right-letter-right-place match, and
        // do NOT want the FIRST 'e' to be scored as a right-letter-wrong-place
        // match.
        //
        // When we find a match, add a 2 in the corresponding place in
        // the score.
        if *guess == *target {
            result += 2;
            guess_used |= 1;
        }

        if *guess.add(1) == *target.add(1) {
            result += 6;
            guess_used |= 2;
        }

        if *guess.add(2) == *target.add(2) {
            result += 18;
            guess_used |= 4;
        }

        if *guess.add(3) == *target.add(3) {
            result += 54;
            guess_used |= 8;
        }

        if *guess.add(4) == *target.add(4) {
            result += 162;
            guess_used |= 16;
        }

        // Now match the remaining letters, searching for other places.
        // Here we have to consider 5*4 pairings.
        //
        // When we find a match, add a 1 in the corresponding place in
        // the score.
        let mut target_used = guess_used;

        if (guess_used & 1) == 0 {
            let g = *guess.add(0);
            if *target.add(1) == g && (target_used & 2) == 0 {
                target_used |= 2;
                result += 1;
            } else if *target.add(2) == g && (target_used & 4) == 0 {
                target_used |= 4;
                result += 1;
            } else if *target.add(3) == g && (target_used & 8) == 0 {
                target_used |= 8;
                result += 1;
            } else if *target.add(4) == g && (target_used & 16) == 0 {
                target_used |= 16;
                result += 1;
            }
        }

        if (guess_used & 2) == 0 {
            let g = *guess.add(1);
            if *target.add(0) == g && (target_used & 1) == 0 {
                target_used |= 1;
                result += 3;
            } else if *target.add(2) == g && (target_used & 4) == 0 {
                target_used |= 4;
                result += 3;
            } else if *target.add(3) == g && (target_used & 8) == 0 {
                target_used |= 8;
                result += 3;
            } else if *target.add(4) == g && (target_used & 16) == 0 {
                target_used |= 16;
                result += 3;
            }
        }

        if (guess_used & 4) == 0 {
            let g = *guess.add(2);
            if *target.add(0) == g && (target_used & 1) == 0 {
                target_used |= 1;
                result += 9;
            } else if *target.add(1) == g && (target_used & 2) == 0 {
                target_used |= 2;
                result += 9;
            } else if *target.add(3) == g && (target_used & 8) == 0 {
                target_used |= 8;
                result += 9;
            } else if *target.add(4) == g && (target_used & 16) == 0 {
                target_used |= 16;
                result += 9;
            }
        }

        if (guess_used & 8) == 0 {
            let g = *guess.add(3);
            if *target.add(0) == g && (target_used & 1) == 0 {
                target_used |= 1;
                result += 27;
            } else if *target.add(1) == g && (target_used & 2) == 0 {
                target_used |= 2;
                result += 27;
            } else if *target.add(2) == g && (target_used & 4) == 0 {
                target_used |= 4;
                result += 27;
            } else if *target.add(4) == g && (target_used & 16) == 0 {
                target_used |= 16;
                result += 27;
            }
        }

        if (guess_used & 16) == 0 {
            let g = *guess.add(4);
            if (*target.add(0) == g && (target_used & 1) == 0)
                || (*target.add(1) == g && (target_used & 2) == 0)
                || (*target.add(2) == g && (target_used & 4) == 0)
                || (*target.add(3) == g && (target_used & 8) == 0)
            {
                result += 81;
            }
        }
    }

    debug_assert_eq!(score_word_pair_simple(guess, target), result, "Optimized version of score_word_pair generated a different score from the simple version. guess='{}' target='{}'", guess, target);

    result
}

// While WordScore represents how a guessed word compares to a single target word,
// GuessQuality represents how a guessed word compares against an entire list of
// possible targets.
//
// The basic logic is to take a list of candidate guess words, then generate a GuessQuality
// for each of them against the list of candidate solution words, and then sort/select
// among the GuessQualities to suggest good guesses.
struct GuessQuality<'a> {
    has_winning: bool,
    expected_remaining: f64,
    max_remaining: u16,
    score_with_max_remaining: u8,
    guess: &'a str,
}

// Score a single candidate guess word against the list of remaining words.
fn estimate_guess_quality<'a>(guess: &'a str, targets: &[&str]) -> GuessQuality<'a> {
    let mut histogram: [u16; NUM_SCORES] = [0u16; NUM_SCORES];
    for &target in targets {
        let score = score_word_pair(guess, target);
        histogram[score as usize] += 1;
    }

    let mut max_with_score = 0u16;
    let mut score_with_max = 0u8;
    let mut expected = 0u64;
    for score in 0..NUM_SCORES {
        let num_with_score = histogram[score];
        if num_with_score > max_with_score {
            max_with_score = num_with_score;
            score_with_max = score as u8;
        }
        expected += num_with_score as u64 * num_with_score as u64;
    }

    GuessQuality {
        has_winning: histogram[242] > 0,
        expected_remaining: expected as f64 / targets.len() as f64,
        max_remaining: max_with_score,
        score_with_max_remaining: score_with_max,
        guess: guess,
    }
}

// Print a presorted GuessQuality list in a way that's user-friendly.
fn print_suggested_guess_list(list: &Vec<GuessQuality>, targets: &[&str]) {
    let mut num_winning = 0;
    let mut num_skipped = 0;
    for (i, q) in list.iter().enumerate() {
        let max_targets_shown = 10;
        let targets_with_max_score = targets
            .into_iter()
            .copied()
            .filter(|w| score_word_pair(q.guess, w) == q.score_with_max_remaining)
            .take(max_targets_shown + 1)
            .collect::<Vec<&str>>();

        if i < 15 || q.has_winning {
            if num_skipped > 0 {
                println!("   ... ({} words omitted) ...", num_skipped);
                num_skipped = 0;
            }

            println!(
                "{} {} | average {:.1} left, max {} left with {} => {}{}",
                if q.has_winning { '*' } else { ' ' },
                q.guess,
                q.expected_remaining,
                q.max_remaining,
                format_score(q.score_with_max_remaining),
                targets_with_max_score
                    .iter()
                    .take(max_targets_shown)
                    .copied()
                    .collect::<Vec<&str>>()
                    .join(" "),
                if targets_with_max_score.len() > max_targets_shown {
                    "..."
                } else {
                    ""
                }
            );
        } else {
            num_skipped += 1;
        }

        if q.has_winning {
            num_winning += 1;
        }

        if num_winning > 4 && i > 10 {
            break;
        }
    }
}

// The core routine. Check the quality of various guesses against the full set
// of targets, sort the qualities in a useful way, and print them out.
fn generate_and_print_suggestions(guesses: &[&str], targets: &[&str]) {
    let mut all_guesses_scored: Vec<_> = guesses
        .into_par_iter() // why is this so much faster than .par_iter()?
        .map(|w| estimate_guess_quality(w, targets))
        .collect();

    println!("\nSUGGESTED GUESSES (sorted by expected_remaining * max_remaining)\n======================================================================================================");
    all_guesses_scored.sort_by(|a, b| {
        // Primary sort works best when we multiply these together.
        let aprod = a.max_remaining as f64 * a.expected_remaining;
        let bprod = b.max_remaining as f64 * b.expected_remaining;
        let o = aprod.partial_cmp(&bprod);
        if matches!(o, Some(Ordering::Greater | Ordering::Less)) {
            return o.unwrap();
        }

        // Break ties by favoring things that might win!
        let o = b.has_winning.cmp(&a.has_winning);
        if matches!(o, Ordering::Greater | Ordering::Less) {
            return o;
        }

        // Break ties by favoring things that are guaranteed to cull the most.
        let o = a.max_remaining.cmp(&b.max_remaining);
        if matches!(o, Ordering::Greater | Ordering::Less) {
            return o;
        }

        // Break ties by favoring things that will cull the most on average.
        let o = a.expected_remaining.partial_cmp(&b.expected_remaining);
        if matches!(o, Some(Ordering::Greater | Ordering::Less)) {
            return o.unwrap();
        }

        // Break ties alphabetically.
        a.guess.cmp(&b.guess)
    });

    print_suggested_guess_list(&all_guesses_scored, targets);
}

#[derive(Parser)]
#[clap(version, long_about = "This is a small console program (primarily) for me to learn Rust, and (secondarily) to suggest good words for the web-based word-guessing game Wordle at https://www.powerlanguage.co.uk/wordle/\n\nSee documentation and example usage at https://github.com/meggsOmatic/WordleGuesser")]
struct CmdArgs {
    /// Hard Mode: If you play Wordle with this turned on from its settings,
    /// then once you correctly guess a letter, Wordle will require that you use it in all later guesses.
    #[clap(short, long)]
    hard: bool,

    /// Normally, the 5000 most-common 5-letter English words are used as the starting point
    /// for your guesses. You can increase the size of that list to get some less-common words, or increase
    /// it to to only use the most common.
    #[clap(short, long, conflicts_with="solutions")]
    common: Option<u32>,

    /// Use the Wordle solution list. Normally, wordle_solver will come up with guesses that narrow down
    /// a list of the most common 5-letter English words. If you specify this option, it will instead use
    /// Wordle's actual set of solution words as the starting point. Guesses will be more accurate, but
    /// doesn't this feel like cheating to you?
    #[clap(short, long)]
    solutions: bool,
}

fn main() {
    let cmd_args = CmdArgs::parse();

    // These are the words that Wordle considers valid guesses. It appears to be based on a
    // Scrabble word list. While nearly all of these are in my dictionary, some are so obscure,
    // so archaic, or so limited to specific technical contexts that no reasonable puzzle
    // creator would use them for something the general public is expected to solve. But
    // they're still useful as possible guesses -- even if a word is unlikely to be the used
    // in writing or as the solution to a puzzle, its pattern of letters may be really effective
    // at narrowing down the possible solutions.
    //
    // If we're playing in "hard mode", we'll shrink this list with each guess, so that you
    // can only guess words that fit with your previous guesses. For normal mode we'll leave
    // this entire list for consideration -- a word that won't win can sometimes be really
    // effective at narrowing the possibilities for the target word.
    let mut valid_guesses: Vec<&str> = scrabble_word_list::SCRABBLE_WORD_LIST.to_vec();

    // These are the words that are under consideration as possible solutions. It begins
    // as a list of valid words that are in common enough usage that they could reasonably
    // be chosen as the target word. With each guess, we'll cull the list of things that
    // don't match the score for that guess.
    let mut remaining_targets: Vec<&str> = if cmd_args.solutions {
        let frequency_hash: HashMap<&str, u32> = word_frequency_list::WORD_FREQUENCY_LIST
            .into_iter()
            .copied()
            .collect();
        wordle_solutions::WORDLE_SOLUTION_LIST
            .iter()
            .map(|w| (u32::MAX - frequency_hash.get(w).copied().unwrap_or_default(), *w))
            .sorted()
            .map(|(_freq, word)| word)
            .collect::<Vec<&str>>()
    } else {
        // The word frequency list is based on an analysis of in-the-wild English texts, so it
        // includes acronyms, proper names, common typos and misspellings, perhaps OCR errors,
        // etc. We filter it against the list of valid guesses to cull out things that Wordle
        // doesn't consider words.
        //
        // Interesting note: If we filtered in the opposite direction, only about 2/3 of the
        // "valid" words in the Scrabble word list are common enough to appear in the word
        // frequency list at all!
        let valid_guesses_hash: HashSet<&str> = HashSet::from_iter(valid_guesses.iter().copied());

        // We also set a threshold of frequency, since the whole point of this is to limit the
        // possible targets to words that are in common-enough usage that they might reasonably
        // be chosen for a puzzle that the general public is expected to solve. A threshold of
        // 1/20000 as common as the most-common word gives us just under 6000 words, with the
        // the least-common being words like "yenta" and "cardy".
        word_frequency_list::WORD_FREQUENCY_LIST
            .iter()
            .filter_map(|(word, _freq)| {
                if valid_guesses_hash.contains(word) {
                    Some(*word)
                } else {
                    None
                }
            })
            .take(cmd_args.common.unwrap_or(5000) as usize)
            .collect()
    };

    // Guess words until we've sufficiently narrowed the space!
    loop {
        // Give some info on the current state of the possibility space.
        match remaining_targets.len() {
            0 => {
                println!("Somehow, there are no possible words remaining. Did you enter your guesses and scores correctly?");
                break;
            }
            1 => {
                println!("The word is: {}", remaining_targets[0]);
                break;
            }
            _ => {
                let max_shown = 200;
                let mut shown = remaining_targets
                    .iter()
                    .take(max_shown)
                    .copied()
                    .collect::<Vec<&str>>()
                    .join(" ");
                if remaining_targets.len() > max_shown {
                    shown.push_str("...");
                }

                println!(
                    "There are {} possibilities for the word.\n\n{}",
                    remaining_targets.len(),
                    textwrap::fill(&shown, textwrap::Options::with_termwidth())
                );

                if remaining_targets.len() == 2 {
                    // If there are only two possible solutions left then you know what to do from here.
                    // Guess one of them, and if it's not that it's the other.
                    break;
                }
            }
        }

        // Analyze the list of remaining words and print out some suggested guesses that will
        // do the most to cull the possibility space, and print them out.
        generate_and_print_suggestions(&valid_guesses, &remaining_targets);

        // Get the word that the user is going to enter and solve the puzzle.
        let guess = loop {
            print!("\nPlease enter the guess you'll use: ");
            io::stdout().flush().expect("Output stream is broken.");

            let mut input_str = String::new();
            io::stdin()
                .read_line(&mut input_str)
                .expect("failed to read");

            input_str = input_str.trim().to_lowercase();
            if input_str.len() == WORD_LENGTH && input_str.chars().all(|c| c.is_alphabetic()) {
                break input_str;
            }

            println!(
                "\nYour guess of '{}' was not exactly five letters.",
                input_str
            );
        };

        // Get the score that the puzzle gave to the user.
        let score = loop {
            print!("Enter the score you got for that word, in \".y.GG\" format: ");
            io::stdout().flush().expect("Output stream is broken.");

            let mut input_str = String::new();
            io::stdin()
                .read_line(&mut input_str)
                .expect("failed to read");

            if let Some(s) = parse_score(input_str.trim()) {
                break s;
            }

            println!("");
            println!(
                "Scores should be entered as {} characters, with this code:",
                WORD_LENGTH
            );
            println!("  . = letter that did not matching anything");
            println!("  y = (yellow) letter that's in the word but in the wrong place");
            println!("  G = (GREEN) the right letter in the right place");
            println!("");
        };

        // Cull the solution space to things that would give the above score for the above guess.
        remaining_targets.retain(|w| score_word_pair(&guess, w) == score);

        // If we're in hard mode, cull the list of valid guesses as well.
        if cmd_args.hard {
            valid_guesses.retain(|w| score_word_pair(&guess, w) == score);
        }
    }
}
