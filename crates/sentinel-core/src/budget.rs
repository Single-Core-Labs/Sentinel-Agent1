use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BudgetReservation {
    pub reservation_id: String,
    pub estimated_cost_usd: f64,
    pub actual_cost_usd: Option<f64>,
    pub reconciled: bool,
}

#[derive(Debug, Clone)]
pub struct BudgetGuard {
    /// Per-session cost cap in USD. None = unlimited.
    pub cost_cap_usd: Option<f64>,
    /// Running total of confirmed spend in USD.
    pub total_spend_usd: f64,
    /// Running total of reserved (estimated but not yet reconciled) spend.
    pub reserved_spend_usd: f64,
    /// Active reservations keyed by reservation_id.
    reservations: HashMap<String, BudgetReservation>,
    /// Whether auto-approval mode is enabled (yolo).
    pub auto_approval_enabled: bool,
    /// Whether the budget has been exhausted (triggers denial of new reservations).
    pub exhausted: bool,
}

impl BudgetGuard {
    pub fn new(cost_cap_usd: Option<f64>, auto_approval_enabled: bool) -> Self {
        Self {
            cost_cap_usd,
            total_spend_usd: 0.0,
            reserved_spend_usd: 0.0,
            reservations: HashMap::new(),
            auto_approval_enabled,
            exhausted: false,
        }
    }

    /// Total estimated spend including reservations + confirmed.
    pub fn estimated_total(&self) -> f64 {
        self.total_spend_usd + self.reserved_spend_usd
    }

    /// Remaining budget before hitting the cap. None if unlimited.
    pub fn remaining_usd(&self) -> Option<f64> {
        self.cost_cap_usd.map(|cap| (cap - self.estimated_total()).max(0.0))
    }

    /// Reserve budget for an operation with an estimated cost.
    /// Returns false if the reservation would exceed the cap.
    pub fn reserve(&mut self, reservation_id: &str, estimated_cost_usd: f64) -> bool {
        if self.exhausted {
            return false;
        }
        if let Some(cap) = self.cost_cap_usd {
            let new_total = self.estimated_total() + estimated_cost_usd;
            if new_total > cap {
                return false;
            }
        }
        self.reserved_spend_usd += estimated_cost_usd;
        self.reservations.insert(
            reservation_id.to_string(),
            BudgetReservation {
                reservation_id: reservation_id.to_string(),
                estimated_cost_usd,
                actual_cost_usd: None,
                reconciled: false,
            },
        );
        true
    }

    /// Reconcile a reservation with the actual cost.
    /// Adjusts reserved_spend_usd by the difference between estimate and actual.
    pub fn reconcile(&mut self, reservation_id: &str, actual_cost_usd: f64) {
        if let Some(reservation) = self.reservations.get_mut(reservation_id) {
            if !reservation.reconciled {
                let diff = actual_cost_usd - reservation.estimated_cost_usd;
                self.reserved_spend_usd = (self.reserved_spend_usd + diff).max(0.0);
                reservation.actual_cost_usd = Some(actual_cost_usd);
                reservation.reconciled = true;
            }
        }
    }

    /// Confirm a reservation as final spend — moves it from reserved to confirmed.
    pub fn confirm(&mut self, reservation_id: &str, final_cost_usd: f64) {
        self.reconcile(reservation_id, final_cost_usd);
        if let Some(reservation) = self.reservations.get(reservation_id) {
            if reservation.reconciled {
                self.reserved_spend_usd = (self.reserved_spend_usd - final_cost_usd).max(0.0);
                self.total_spend_usd += final_cost_usd;
                self.reservations.remove(reservation_id);
            }
        }
    }

    /// Record confirmed spend directly (no prior reservation).
    pub fn record_spend(&mut self, cost_usd: f64) {
        self.total_spend_usd += cost_usd;
        if let Some(cap) = self.cost_cap_usd {
            if self.total_spend_usd >= cap {
                self.exhausted = true;
            }
        }
    }

    /// Check if a proposed cost is within budget.
    pub fn would_exceed_cap(&self, cost_usd: f64) -> bool {
        self.cost_cap_usd
            .map(|cap| self.estimated_total() + cost_usd > cap)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlimited_budget() {
        let mut guard = BudgetGuard::new(None, true);
        assert!(guard.reserve("r1", 100.0));
        assert!(guard.reserve("r2", 500.0));
        assert_eq!(guard.estimated_total(), 600.0);
    }

    #[test]
    fn test_capped_budget_reserves() {
        let mut guard = BudgetGuard::new(Some(10.0), true);
        assert!(guard.reserve("r1", 5.0));
        assert!(guard.reserve("r2", 4.0));
        assert!(!guard.reserve("r3", 2.0)); // would exceed cap
        assert_eq!(guard.estimated_total(), 9.0);
        assert_eq!(guard.remaining_usd(), Some(1.0));
    }

    #[test]
    fn test_reconcile_adjusts_reserved() {
        let mut guard = BudgetGuard::new(Some(10.0), true);
        guard.reserve("r1", 5.0);
        guard.reconcile("r1", 3.0); // actual < estimate
        assert_eq!(guard.reserved_spend_usd, 3.0);
    }

    #[test]
    fn test_confirm_moves_to_total() {
        let mut guard = BudgetGuard::new(Some(10.0), true);
        guard.reserve("r1", 5.0);
        guard.confirm("r1", 4.5);
        assert_eq!(guard.total_spend_usd, 4.5);
        assert_eq!(guard.reserved_spend_usd, 0.0);
    }

    #[test]
    fn test_exhausted_flag() {
        let mut guard = BudgetGuard::new(Some(5.0), true);
        guard.record_spend(5.0);
        assert!(guard.exhausted);
        assert!(!guard.reserve("r1", 1.0));
    }
}
