use std::cmp::Reverse;

use crate::AccId;

/// Always buy at lowest price
#[derive(Debug)]
pub struct Bid {
    pub price: Reverse<ordered_float::NotNan<f64>>,
    pub account: AccId,
}

impl Bid {
    pub fn new(acc: AccId, price: f64) -> Self {
        Self {
            price: Reverse(ordered_float::NotNan::new(price).unwrap()),
            account: acc,
        }
    }
}

impl PartialEq for Bid {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl PartialOrd for Bid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Eq for Bid {}

impl Ord for Bid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price.cmp(&other.price)
    }
}

/// Always sell at highest price
#[derive(Debug)]
pub struct Ask {
    pub price: ordered_float::NotNan<f64>,
    pub account: AccId,
}

impl Ask {
    pub fn new(acc: AccId, price: f64) -> Self {
        Self {
            price: ordered_float::NotNan::new(price).unwrap(),
            account: acc,
        }
    }
}

impl PartialEq for Ask {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl PartialOrd for Ask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Eq for Ask {}

impl Ord for Ask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price.cmp(&other.price)
    }
}
