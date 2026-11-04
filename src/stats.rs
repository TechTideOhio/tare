use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McNemarResult {
    pub concordant_pass: usize,
    pub concordant_fail: usize,
    pub regressions: usize,
    pub gains: usize,
    pub total_discordant: usize,
    pub p_value: f64,
    pub is_significant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapCi {
    pub point_estimate: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}

pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn next_usize(&mut self, upper_bound: usize) -> usize {
        if upper_bound <= 1 {
            return 0;
        }
        (self.next_u64() % (upper_bound as u64)) as usize
    }
}

pub fn ln_gamma(x: f64) -> f64 {
    let p = [
        676.520_368_121_885_1,
        -1259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_849_084_9,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    if x < 0.5 {
        std::f64::consts::PI.ln() - (std::f64::consts::PI * x).sin().ln() - ln_gamma(1.0 - x)
    } else {
        let z = x - 1.0;
        let mut base = 0.999_999_999_999_809_9;
        for (i, p_val) in p.iter().enumerate() {
            base += p_val / (z + (i as f64) + 1.0);
        }
        let t = z + 7.5;
        0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + base.ln()
    }
}

pub fn ln_choose(n: usize, k: usize) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    if k == 0 || k == n {
        return 0.0;
    }
    ln_gamma((n as f64) + 1.0) - ln_gamma((k as f64) + 1.0) - ln_gamma(((n - k) as f64) + 1.0)
}

pub fn exact_mcnemar_test(
    concordant_pass: usize,
    concordant_fail: usize,
    regressions: usize,
    gains: usize,
    alpha: f64,
) -> McNemarResult {
    let n_d = regressions + gains;
    let p_value = if n_d == 0 {
        1.0
    } else {
        let k_min = regressions.min(gains);
        let mut tail_prob = 0.0;
        let n_d_f = n_d as f64;
        let log_half = 0.5f64.ln();

        for i in 0..=k_min {
            let log_prob = ln_choose(n_d, i) + n_d_f * log_half;
            tail_prob += log_prob.exp();
        }

        (2.0 * tail_prob).min(1.0)
    };

    let is_significant = p_value < alpha;

    McNemarResult {
        concordant_pass,
        concordant_fail,
        regressions,
        gains,
        total_discordant: n_d,
        p_value,
        is_significant,
    }
}

pub fn check_tost_equivalence(pass_rate_a: f64, pass_rate_b: f64, margin: f64) -> bool {
    let delta = pass_rate_b - pass_rate_a;
    delta.abs() <= margin
}

#[derive(Debug, Clone)]
pub struct PairedObservation {
    pub baseline_passed: bool,
    pub candidate_passed: bool,
    pub baseline_cost: f64,
    pub candidate_cost: f64,
    pub baseline_duration_ms: f64,
    pub candidate_duration_ms: f64,
    pub baseline_tokens: u64,
    pub candidate_tokens: u64,
}

pub fn bootstrap_paired_deltas(
    pairs: &[PairedObservation],
    iterations: usize,
    seed: u64,
) -> (BootstrapCi, BootstrapCi, BootstrapCi, BootstrapCi) {
    let n = pairs.len();
    if n == 0 {
        let zero = BootstrapCi {
            point_estimate: 0.0,
            ci_lower: 0.0,
            ci_upper: 0.0,
        };
        return (zero.clone(), zero.clone(), zero.clone(), zero);
    }

    let b_passed = pairs.iter().filter(|p| p.baseline_passed).count();
    let c_passed = pairs.iter().filter(|p| p.candidate_passed).count();
    let b_qual = (b_passed as f64) / (n as f64);
    let c_qual = (c_passed as f64) / (n as f64);
    let point_qual_delta = c_qual - b_qual;

    let b_cost_sum: f64 = pairs.iter().map(|p| p.baseline_cost).sum();
    let c_cost_sum: f64 = pairs.iter().map(|p| p.candidate_cost).sum();
    let b_cpst = if b_passed > 0 {
        b_cost_sum / (b_passed as f64)
    } else {
        0.0
    };
    let c_cpst = if c_passed > 0 {
        c_cost_sum / (c_passed as f64)
    } else {
        0.0
    };
    let point_cpst_pct = if b_cpst > 0.000_001 {
        ((c_cpst - b_cpst) / b_cpst) * 100.0
    } else {
        0.0
    };

    let mut b_lats: Vec<_> = pairs.iter().map(|p| p.baseline_duration_ms).collect();
    let mut c_lats: Vec<_> = pairs.iter().map(|p| p.candidate_duration_ms).collect();
    b_lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    c_lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let b_p95 = crate::metrics::calculate_quantile(&b_lats, 0.95);
    let c_p95 = crate::metrics::calculate_quantile(&c_lats, 0.95);
    let point_p95_pct = if b_p95 > 0.001 {
        ((c_p95 - b_p95) / b_p95) * 100.0
    } else {
        0.0
    };

    let b_tok_sum: u64 = pairs.iter().map(|p| p.baseline_tokens).sum();
    let c_tok_sum: u64 = pairs.iter().map(|p| p.candidate_tokens).sum();
    let b_mean_tok = (b_tok_sum as f64) / (n as f64);
    let c_mean_tok = (c_tok_sum as f64) / (n as f64);
    let point_tok_pct = if b_mean_tok > 0.1 {
        ((c_mean_tok - b_mean_tok) / b_mean_tok) * 100.0
    } else {
        0.0
    };

    let mut rng = SplitMix64::new(seed);
    let mut qual_samples = Vec::with_capacity(iterations);
    let mut cpst_pct_samples = Vec::with_capacity(iterations);
    let mut p95_pct_samples = Vec::with_capacity(iterations);
    let mut tok_pct_samples = Vec::with_capacity(iterations);

    let mut resample_b_lats = Vec::with_capacity(n);
    let mut resample_c_lats = Vec::with_capacity(n);

    for _ in 0..iterations {
        let mut r_b_passed = 0;
        let mut r_c_passed = 0;
        let mut r_b_cost = 0.0;
        let mut r_c_cost = 0.0;
        let mut r_b_tokens = 0u64;
        let mut r_c_tokens = 0u64;
        resample_b_lats.clear();
        resample_c_lats.clear();

        for _ in 0..n {
            let idx = rng.next_usize(n);
            let item = &pairs[idx];
            if item.baseline_passed {
                r_b_passed += 1;
            }
            if item.candidate_passed {
                r_c_passed += 1;
            }
            r_b_cost += item.baseline_cost;
            r_c_cost += item.candidate_cost;
            r_b_tokens += item.baseline_tokens;
            r_c_tokens += item.candidate_tokens;
            resample_b_lats.push(item.baseline_duration_ms);
            resample_c_lats.push(item.candidate_duration_ms);
        }

        let q_delta = ((r_c_passed as f64) - (r_b_passed as f64)) / (n as f64);
        qual_samples.push(q_delta);

        let r_b_cpst = if r_b_passed > 0 {
            r_b_cost / (r_b_passed as f64)
        } else {
            0.0
        };
        let r_c_cpst = if r_c_passed > 0 {
            r_c_cost / (r_c_passed as f64)
        } else {
            0.0
        };
        let cpst_delta_pct = if r_b_cpst > 0.000_001 {
            ((r_c_cpst - r_b_cpst) / r_b_cpst) * 100.0
        } else {
            0.0
        };
        cpst_pct_samples.push(cpst_delta_pct);

        resample_b_lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        resample_c_lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rb_p95 = crate::metrics::calculate_quantile(&resample_b_lats, 0.95);
        let rc_p95 = crate::metrics::calculate_quantile(&resample_c_lats, 0.95);
        let p95_delta_pct = if rb_p95 > 0.001 {
            ((rc_p95 - rb_p95) / rb_p95) * 100.0
        } else {
            0.0
        };
        p95_pct_samples.push(p95_delta_pct);

        let tok_delta_pct = if r_b_tokens > 0 {
            (((r_c_tokens as f64) - (r_b_tokens as f64)) / (r_b_tokens as f64)) * 100.0
        } else {
            0.0
        };
        tok_pct_samples.push(tok_delta_pct);
    }

    qual_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cpst_pct_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    p95_pct_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    tok_pct_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let qual_ci = BootstrapCi {
        point_estimate: point_qual_delta,
        ci_lower: crate::metrics::calculate_quantile(&qual_samples, 0.025),
        ci_upper: crate::metrics::calculate_quantile(&qual_samples, 0.975),
    };

    let cpst_ci = BootstrapCi {
        point_estimate: point_cpst_pct,
        ci_lower: crate::metrics::calculate_quantile(&cpst_pct_samples, 0.025),
        ci_upper: crate::metrics::calculate_quantile(&cpst_pct_samples, 0.975),
    };

    let p95_ci = BootstrapCi {
        point_estimate: point_p95_pct,
        ci_lower: crate::metrics::calculate_quantile(&p95_pct_samples, 0.025),
        ci_upper: crate::metrics::calculate_quantile(&p95_pct_samples, 0.975),
    };

    let tok_ci = BootstrapCi {
        point_estimate: point_tok_pct,
        ci_lower: crate::metrics::calculate_quantile(&tok_pct_samples, 0.025),
        ci_upper: crate::metrics::calculate_quantile(&tok_pct_samples, 0.975),
    };

    (qual_ci, cpst_ci, p95_ci, tok_ci)
}
