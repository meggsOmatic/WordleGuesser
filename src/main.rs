mod wordlist;
use rayon::prelude::*;
use std::collections::*;

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

const NUM_SCORES: usize = 243; // This is 3^WORD_LENGTH
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


// Calculate the score for a given guess against a given target. Note that this is NOT symmetric.
// i.e.  score_word_pair("caddy", "abbey") != score_word_pair("abbey", "caddy")
fn score_word_pair(guess: &str, target: &str) -> WordScore {
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
            result = result + (2 * mult);
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


struct TwoGuessQuality<'a> {
    expected_remaining: f64,
    max_remaining: u16,
    guess0: &'a str,
    guess1: &'a str,
}

fn estimate_two_guess_quality<'a>(guess0: &'a str, guess1: &'a str, targets: &[&str]) -> TwoGuessQuality<'a> {
    let mut histogram: [u16; NUM_SCORES * NUM_SCORES] = [0u16; NUM_SCORES * NUM_SCORES];
    for &target in targets {
        let score0 = score_word_pair(guess0, target);
        let score1 = score_word_pair(guess1, target);
        histogram[score0 as usize * NUM_SCORES + score1 as usize] += 1;
    }

    let mut max_with_score = 0u16;
    let mut expected = 0u64;
    for score in 0..NUM_SCORES*NUM_SCORES {
        let num_with_score = histogram[score];
        if num_with_score > max_with_score {
            max_with_score = num_with_score;
        }
        expected += num_with_score as u64 * num_with_score as u64;
    }

    TwoGuessQuality {
        expected_remaining: expected as f64 / targets.len() as f64,
        max_remaining: max_with_score,
        guess0, guess1
    }
}

fn main() {
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
    let valid_guesses: Vec<&str> = wordlist::SCRABBLE_WORD_LIST.to_vec();

    // These are the words that are under consideration as possible solutions. It begins
    // as a list of valid words that are in common enough usage that they could reasonably
    // be chosen as the target word. With each guess, we'll cull the list of things that
    // don't match the score for that guess.
    let remaining_targets: Vec<&str> = {
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
        let minimum_frequency = wordlist::WORD_FREQUENCY_LIST[0].1 / 20000;
        wordlist::WORD_FREQUENCY_LIST
            .iter()
            .take_while(|(_word, freq)| *freq > minimum_frequency)
            .filter_map(|(word, _freq)| {
                if valid_guesses_hash.contains(word) {
                    Some(*word)
                } else {
                    None
                }
            })
            .collect()
    };

    let mut pairs: Vec<(&str, &str)> = Vec::new();
    pairs.push(("grant", "soled"));

    let all_guesses_scored: Vec<_> = pairs
        .into_par_iter() // why is this so much faster than .par_iter()?
        .map(|(guess0, guess1)| estimate_two_guess_quality(guess0, guess1, &remaining_targets))
        .collect();

    println!("{}", all_guesses_scored[0].max_remaining);

}
