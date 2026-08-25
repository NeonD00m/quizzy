use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rating {
    Again = 1, // Forgot
    Hard = 2,  // Recalled with effort
    Good = 3,  // Standard successful recall
    Easy = 4,  // Immediate recall
}

impl Rating {
    pub fn score_delta(&self) -> i64 {
        match self {
            Rating::Again => -3,
            Rating::Hard => -1,
            Rating::Good => 1,
            Rating::Easy => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CardState {
    New = 0,
    Learning = 1,
    Review = 2,
    Relearning = 3,
}

impl CardState {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => CardState::Learning,
            2 => CardState::Review,
            3 => CardState::Relearning,
            _ => CardState::New,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FsrsCard {
    pub stability: f64,
    pub difficulty: f64,
    pub last_review: i64, // Epoch seconds
    pub reps: u32,
    pub lapses: u32,
    pub state: CardState,
}

impl Default for FsrsCard {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            last_review: 0,
            reps: 0,
            lapses: 0,
            state: CardState::New,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingInfo {
    pub card: FsrsCard,
    pub interval_days: i64,
    pub next_due: i64, // Epoch seconds
}

/// Standard default FSRS v4.5/v6 parameter array (19 parameters)
pub const DEFAULT_WEIGHTS: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1334, 0.5345, 1.3461, 0.0, 1.0171, 1.98, 0.0953, 0.2975,
    0.472, 0.2407, 0.2947, 0.25, 2.9466, 0.5, 0.6,
];

pub struct FsrsEngine {
    pub w: [f64; 19],
    pub desired_retention: f64,
}

impl Default for FsrsEngine {
    fn default() -> Self {
        Self::new(0.9)
    }
}

impl FsrsEngine {
    pub fn new(desired_retention: f64) -> Self {
        Self {
            w: DEFAULT_WEIGHTS,
            desired_retention,
        }
    }

    /// Retrievability R(t, S) using power curve
    pub fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        let factor = 19.0 / 81.0;
        (1.0 + factor * (elapsed_days / stability)).powf(-0.5)
    }

    /// Calculates the interval in days required to hit desired_retention
    pub fn next_interval(&self, stability: f64) -> i64 {
        if stability <= 0.0 {
            return 1;
        }
        let factor = 19.0 / 81.0;
        let days = (stability / factor) * (self.desired_retention.powf(-2.0) - 1.0);
        days.round().max(1.0) as i64
    }

    /// Initial difficulty upon first rating
    fn init_difficulty(&self, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let d = self.w[4] - (g - 3.0) * self.w[5];
        d.clamp(1.0, 10.0)
    }

    /// Initial stability upon first rating
    fn init_stability(&self, rating: Rating) -> f64 {
        let g = rating as usize;
        self.w[g - 1].max(0.1)
    }

    /// Difficulty update for existing cards
    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let g = rating as u8 as f64;
        let next_d = self.w[7] * d + (1.0 - self.w[7]) * (self.w[4] - (g - 3.0) * self.w[5]);
        next_d.clamp(1.0, 10.0)
    }

    /// New Stability on Successful Recall (Good / Hard / Easy)
    fn next_recall_stability(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard {
            self.w[15]
        } else {
            1.0
        };
        let easy_bonus = if rating == Rating::Easy {
            self.w[16]
        } else {
            1.0
        };

        let s_inc = (self.w[8].exp())
            * (11.0 - d)
            * s.powf(-self.w[9])
            * (((1.0 - r) * self.w[10]).exp() - 1.0)
            * hard_penalty
            * easy_bonus;

        (s * (1.0 + s_inc)).max(0.1)
    }

    /// New Stability on Forget (Again)
    fn next_forget_stability(&self, d: f64, s: f64, r: f64) -> f64 {
        let next_s = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * (-(1.0 - r) * self.w[14]).exp();
        next_s.clamp(0.1, s)
    }

    /// Processes a review and produces updated card parameters
    pub fn review_card(&self, mut card: FsrsCard, rating: Rating, now_secs: i64) -> SchedulingInfo {
        let elapsed_days = if card.last_review == 0 {
            0.0
        } else {
            ((now_secs - card.last_review) as f64 / 86400.0).max(0.0)
        };

        if card.reps == 0 {
            // First time seeing the card
            card.difficulty = self.init_difficulty(rating);
            card.stability = self.init_stability(rating);
            card.state = if rating == Rating::Again {
                card.lapses += 1;
                CardState::Learning
            } else {
                CardState::Review
            };
        } else {
            let r = self.retrievability(elapsed_days, card.stability);
            card.difficulty = self.next_difficulty(card.difficulty, rating);

            if rating == Rating::Again {
                card.lapses += 1;
                card.stability = self.next_forget_stability(card.difficulty, card.stability, r);
                card.state = CardState::Relearning;
            } else {
                card.stability =
                    self.next_recall_stability(card.difficulty, card.stability, r, rating);
                card.state = CardState::Review;
            }
        }

        card.reps += 1;
        card.last_review = now_secs;

        let interval_days = self.next_interval(card.stability);
        let next_due = now_secs + (interval_days * 86400);

        SchedulingInfo {
            card,
            interval_days,
            next_due,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsrs_initial_review() {
        let engine = FsrsEngine::new(0.9);
        let card = FsrsCard::default();
        let now = 100000;

        let info = engine.review_card(card, Rating::Good, now);
        assert_eq!(info.card.reps, 1);
        assert_eq!(info.card.lapses, 0);
        assert_eq!(info.card.state, CardState::Review);
        assert!(info.card.stability > 0.0);
        assert!(info.card.difficulty >= 1.0 && info.card.difficulty <= 10.0);
        assert!(info.interval_days >= 1);
        assert_eq!(info.next_due, now + (info.interval_days * 86400));
    }

    #[test]
    fn test_fsrs_again_rating_increments_lapses() {
        let engine = FsrsEngine::new(0.9);
        let card = FsrsCard::default();
        let now = 100000;

        let info = engine.review_card(card, Rating::Again, now);
        assert_eq!(info.card.reps, 1);
        assert_eq!(info.card.lapses, 1);
        assert_eq!(info.card.state, CardState::Learning);

        // Second review after 1 day fails
        let info2 = engine.review_card(info.card, Rating::Again, now + 86400);
        assert_eq!(info2.card.reps, 2);
        assert_eq!(info2.card.lapses, 2);
        assert_eq!(info2.card.state, CardState::Relearning);
    }

    #[test]
    fn test_rating_score_deltas() {
        assert_eq!(Rating::Again.score_delta(), -3);
        assert_eq!(Rating::Hard.score_delta(), -1);
        assert_eq!(Rating::Good.score_delta(), 1);
        assert_eq!(Rating::Easy.score_delta(), 3);
    }
}
