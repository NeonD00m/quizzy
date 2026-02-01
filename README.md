# Quizzy
_A quizlet-like cli for practicing locally and saving card-sets faster than ever before!_

### Practice Mode Outputs:
Learn mode:
```
$: quizzy learn example_deck
Term: yeah    (1/30)
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
X: sure       ✓: no
```
Card mode:
```
$: quizzy card example_deck
Term        (space to flip)
╭──────────────╮
│     yeah     │
╰──────────────╯
Definition  (space to flip)
╭──────────────╮
│  definitely  │
╰──────────────╯
// todo: write over the card to show back/front?
```
Gamble mode:
```
$: quizzy gamble example_deck
Would you like to play one at a time or make a parlay:
> Parlay
> Gauntlet <

Instructions:
Type "DOUBLE" during a round to double multiplier gain, "BANK" to save your money, 1-4 to guess, Esc to exit (will lose money!).

========================================
           STUDY CASINO: OPEN
========================================
DECK: example_deck
BANK: 8720
WAGER: 100
-----------------------------------------
> Press [ENTER] to deal the first card...

╭──────────────╮
│     yeah     │
╰──────────────╯
> Double down? Y
What's on the other side? [###############--|--|--|--|--]
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
X: sure       ✓: no

> Play Again? Y

========================================
           STUDY CASINO: OPEN
========================================
DECK: example_deck
BANK: 8520
WAGER: 20
-----------------------------------------
> Press [ENTER] to deal the first card...

╭──────────────╮
│     yeah     │
╰──────────────╯
> Double down? Y
What's on the other side? [--|--|--|--|--|--|--|--|--|--]
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
✓: no

╭──────────────╮
│     yeah     │
╰──────────────╯
What's on the other side? [--|--|--|--|--|--|--|--|--]
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
✓: no

╭──────────────╮
│     yeah     │
╰──────────────╯
What's on the other side? [#######################]
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
Bust! You ran out of time.

> Play Again? N
```
