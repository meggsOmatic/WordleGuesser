# WordleGuesser

This is a console program (primarily) for me to learn Rust, and (secondarily) to suggest good words for the web-based word-guessing game Wordle at https://www.powerlanguage.co.uk/wordle/.

Wordle is a simple fun game. There's a 5-letter secret word chosen each day. You try and guess it. The game gives you feedback about your guess, indicating which letters are in the target word and whether they're in the right place. Use that to refine your next guess until you've successfully guessed the word.

![image](https://user-images.githubusercontent.com/5649419/149420668-5b0e3777-c62c-4bad-91a3-19a1139a5deb.png)

***This program is not Wordle itself.*** This is a program that will suggest good words for playing Wordle. To play Wordle, go to https://www.powerlanguage.co.uk/wordle/.

## How it works / how to use it

WordleGuesser starts with a list of ~12000 potential guesses (from the Scrabble word list), and a list of potential solution words.

There are options for the potential solutions: The default is a list of the ~5000 most-common 5-letter English words, because we're assuming the Wordle creators are nice people who don't pick obscure words like "xylyl". You can adjust that number up or down depending on how obscure you want to get. Alternately, you can start with the exact list of 2315 words that Wordle itself cycles through for its solutions. That's useful if you're interested in exploring some statistics, but to me that feels like cheating (or at least *more* like cheating than using a program in the first place).

WordleGuesser compares all the guess words against all the solution words, and suggests which guesses will do the most to reduce the number of possible solution words.

You pick one of those suggestions and enter it as a guess in the Wordle gmae.

Then you tell WordleGuesser what word you entered, and how Wordle scored your guess.

WordleGuesser uses this information to narrow its list of solution words in response. Any solution word that would NOT have given that score for your guess is eliminated.

Then Wordle loops around, trying all the guess words against the smaller list of solution words, suggesting the new best guess words, and asking you which one you tried and how it was scored. Eventually the list of possible solutions will be down to 1 or 2 words, and then you're done.

## How to enter scores

There's a simple code to use when entering the score:

    . = letter not in solution word
    y = (yellow) letter is in the solution word, but in a different place!
    G = (green) letter is in the solution word in the same place as your guess
    
So if the secret solution word was `ABBEY`, and you guessed `CADDY`, the score for `CADDY` would be `.y..G`:

- `C` gets a `.` because it's not in the solution word.
- `A` gets a `y` because it's highlighted yellow in Wordle, because it's a correct letter but in the wrong place.
- `D` gets a `.` because it's not in the solution word.
- `D` gets a `.` because it's not in the solution word.
- `Y` gets a `G` because it's highlighted green in Wordle, because it's a correct letter in the correct place.

## An example session

The WordleGuesser console output is on the left. Your inputs are circled in red. The Wordle game that you're playing is on the right.

![WordleGuesser](https://user-images.githubusercontent.com/5649419/149418891-684c1ba3-c64d-4c10-8267-e632d296e2ce.png)

The list of suggested guess words has some useful information in it:

- The **asterisk** on the left indicates that the suggestion is a possibly-winning word. Words that don't have the * have no chance of matching all five letters in the right place, but they can still be great at eliminating possibilities. (If you're playing Wordle in hard mode, you're not allowed to use these.)
- The **average words left** is the [expected value](https://en.wikipedia.org/wiki/Expected_value) of the number of words remaining after that guess. For each of the possible scores that guess could receive, we multiply the odds of getting that score by the number of remaining words that would get that score, and add it all up. You want this to be low!
- The **max words left** is the worst case for that guess. If you played that guess, what possible score would leave you with the most words still remaining? This is as bad as it can be, so you also want this to be low!
- The `.y.GG`-looking column tells you what that worst-case score for that guess would be.
- Finally, if you did get that worst-case score, you see a list of what some of the remaining possible words would be.

The list is sorted by the [geometric mean](https://en.wikipedia.org/wiki/Geometric_mean) of the average and max. That provides a good all-around blend of suggesting guesses that will always be pretty good without ever being terrible. You can reliably pick the top suggestions and play a good Wordle game.

However, if you're feeling optimistic, ignore that order and go for the *lowest average* and an *asterisk*. You'll have the best chance of a quick win, but there's also more chance of doing badly. On the other hand, if you want to play conservatively, always pick the word with the *lowest maximum*. Those words may not win as quickly, but you'll never go completely wrong.

## Command-line options
```
USAGE:
    wordle_guesser.exe [OPTIONS]

OPTIONS:
    -c, --common <COMMON>    Normally, the 5000 most-common 5-letter English words are used as the
                             starting point for your guesses. You can increase the size of that
                             list to get some less-common words, or increase it to to only use the
                             most common
                             
    -h, --hard               Hard Mode: If you play Wordle with this turned on from its settings,
                             then once you correctly guess a letter, Wordle will require that you
                             use it in all later guesses
                             
        --help               Print help information
        
    -s, --solutions          Use the Wordle solution list. Normally, wordle_solver will come up with
                             guesses that narrow down a list of the most common 5-letter English
                             words. If you specify this option, it will instead use Wordle's actual
                             set of solution words as the starting point. Guesses will be more
                             accurate, but doesn't this feel like cheating to you?
                             
    -V, --version            Print version information
```

# Why? Learning Rust.

Obviously, playing Wordle yourself is more fun than having a computer program play Wordle for you. I don't expect that anyone will actually use this to play Wordle, but if you do, slide into my DMs and tell me about it.

The point was to write something in Rust. Solving Wordles was a problem that was big enough to stretch my (very limited) Rust skills, but small enough to be within reach of my (very limited) Rust skills. F# is still my happiest place, and C++ is still what pays the bills, but I can see Rust combining the two.

I would ***really*** appreciate feedback from more experienced Rustaceans. The code works, but I suspect a lot of it isn't really idiomatic. I'd love feedback on what the idioms should be. I'm still hazy on exactly what should and shouldn't be a reference, or why the type of variables sometimes gets inferred to be `&&&&str`. I'm still at the stage of adding and removing ampersands and `.copied()` until things compile, without (yet) any deep understanding of exactly why. I may be accidentally making temporary copies of large data structures.

But my code is out here for someone to see, so I hope someone will see it and tell me how to get better.

# Wordle facts

If we score our guesses against the general English word list, the best first word is `SOARE`. It's an archaic word for "young hawk" that's not in my dictionary, but it's in the Scrabble word list so it's fair game. The feedback you'll get by guessing it as your first word will narrow the space of possible solutions by 97.8% on average. In the absolute worst case (if the `A` is a yellow match in the wrong place, and `S`/`O`/`R`/`E` aren't matches at all), `SOARE` will still eliminate 94.6% of the possibilities. In that case your best possible follow-up second guess would be `LINTY`. Even though `LINTY` doesn't use the `A` that matched, it'll eliminate the most possibilities so you can have a great third guess.

If we take advantage of knowing exactly which 2315 words are in Wordle's solution list, the best possible first guess to narrow down those exact words is `RAISE`. On average it'll reduce the list to only 61 possible words (eliminating 97.4% of the possibilities), and in the worst case (if none of the letters match) there'll still only be 168 words left (eliminating 92.7%). If none of the letters match, the best second guess is `COULD`.

There's no such thing as a best general-purpose second guess, because it depends so heavily on what letters matched on the first guess. But, if you want to go blind with a strong all-purpose pair of starter words, `STAIR` and `CLONE` (played in either order) will do the best job of narrowing things down by your third guess. Of course, if you match lots of letters on the first word, you should modify your second guess accordingly! But no matter what, you'll have an average of 4.5 possible words left for your third guess, and a worst-case scenario of only 16 words left.

Alternately, using a starting combo of `CARSE` and `DOILT` (two Scottish colloquial words meaning "low fertile land along a river" and "foolish or stupid") can do slightly better than `STAIR` and `CLONE` on average (only 4.3 words left), but there's a worst case where they leave more words for your third guess. `CARSE` and `DOILT` are the way to go if you feel optimistic and/or wear a kilt.
