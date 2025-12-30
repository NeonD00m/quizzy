# Quizzy
_A quizlet-like cli for practicing locally and saving card-sets faster than ever before!_

### Desired Output:
Learn mode:
```
$: quizzy learn example_deck
Term: yeah    (1/30)
(1) sure      (3) no
(3) nah       (4) definitely
[1-4/quit] > 
X: sure
✓: no
// stretch goal: update with checkmark/text color?
Term: yeah
1. sure no  ✕   2. no  ✓
3. nah          4. definitely
```
Card mode:
```
$: quizzy card example_deck
Term        (Card 1/6)
╭──────────────╮
│     yeah     │
╰──────────────╯
[space/] > Enter something to show back and enter something to show next/prev card?
// stretch goal: write over the card to show back/front
Term        (space to flip)
╭──────────────╮
│     yeah     │
╰──────────────╯
Definition  (space to flip)
╭──────────────╮
│  definitely  │
╰──────────────╯
```
