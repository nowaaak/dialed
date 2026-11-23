use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Comparison {
    pub metric: String,
    pub a_mean: f64,
    pub b_mean: f64,
    pub change: f64,
    pub low: f64,
    pub high: f64,
    pub verdict: &'static str,
    pub cv_a: f64,
    pub cv_b: f64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct BenchComparison {
    pub runs_a: usize,
    pub runs_b: usize,
    pub results: Vec<Comparison>,
    pub warnings: Vec<String>,
}

const T95: [f64; 30] = [
    12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
    2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
    2.052, 2.048, 2.045, 2.042,
];

pub const NOISE_PCT: f64 = 1.0;

fn t95(df: f64) -> f64 {
    if !df.is_finite() || df >= 120.0 {
        return 1.960;
    }
    let whole = df.floor().max(1.0) as usize;
    match whole {
        1..=30 => T95[whole - 1],
        31..=39 => 2.042,
        40..=59 => 2.021,
        60..=119 => 2.000,
        _ => 1.980,
    }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn variance(values: &[f64], m: f64) -> f64 {
    values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() as f64 - 1.0)
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub fn compare(metric: &str, a: &[f64], b: &[f64]) -> Option<Comparison> {
    if a.len() < 2 || b.len() < 2 || a.iter().chain(b).any(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    let la: Vec<f64> = a.iter().map(|v| v.ln()).collect();
    let lb: Vec<f64> = b.iter().map(|v| v.ln()).collect();
    let (ma, mb) = (mean(&la), mean(&lb));
    let (va, vb) = (variance(&la, ma), variance(&lb, mb));
    let (na, nb) = (a.len() as f64, b.len() as f64);
    let (sa, sb) = (va / na, vb / nb);
    let se2 = sa + sb;
    let half = if se2 > 0.0 {
        let df = se2.powi(2) / (sa.powi(2) / (na - 1.0) + sb.powi(2) / (nb - 1.0));
        t95(df) * se2.sqrt()
    } else {
        0.0
    };
    let d = mb - ma;
    let pct = |x: f64| round1((x.exp() - 1.0) * 100.0);
    let (low, high) = (pct(d - half), pct(d + half));
    let verdict = if low > NOISE_PCT {
        "besser"
    } else if high < -NOISE_PCT {
        "schlechter"
    } else if low >= -NOISE_PCT && high <= NOISE_PCT {
        "kein messbarer Unterschied"
    } else {
        "unklar"
    };
    let cv = |values: &[f64]| {
        let m = mean(values);
        round1(variance(values, m).sqrt() / m * 100.0)
    };
    Some(Comparison {
        metric: metric.to_string(),
        a_mean: round1(mean(a)),
        b_mean: round1(mean(b)),
        change: pct(d),
        low,
        high,
        verdict,
        cv_a: cv(a),
        cv_b: cv(b),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_improvement() {
        let a = [200.0, 201.0, 199.0, 200.5, 199.5];
        let b = [220.0, 221.0, 219.0, 220.5, 219.5];
        let c = compare("Ø FPS", &a, &b).unwrap();
        assert_eq!(c.verdict, "besser");
        assert_eq!(c.change, 10.0);
        assert!(c.low > 9.0 && c.high < 11.0);
        assert_eq!(c.a_mean, 200.0);
    }

    #[test]
    fn noise_is_not_a_win() {
        let a = [200.0, 210.0, 190.0];
        let b = [204.0, 214.0, 194.0];
        let c = compare("Ø FPS", &a, &b).unwrap();
        assert_eq!(c.verdict, "unklar");
        assert!(c.low < 0.0 && c.high > 0.0);
    }

    #[test]
    fn identical_runs_show_no_difference() {
        let a = [150.0, 150.2, 149.8, 150.1];
        let b = [150.1, 149.9, 150.0, 150.2];
        assert_eq!(
            compare("Ø FPS", &a, &b).unwrap().verdict,
            "kein messbarer Unterschied"
        );
    }

    #[test]
    fn worse_and_invalid_input() {
        let a = [120.0, 121.0, 119.0];
        let b = [100.0, 101.0, 99.0];
        assert_eq!(compare("1%-Low", &a, &b).unwrap().verdict, "schlechter");
        assert!(compare("x", &[100.0], &b).is_none());
        assert!(compare("x", &[100.0, 0.0], &b).is_none());
    }

    #[test]
    fn t_table_is_conservative() {
        assert_eq!(t95(2.0), 4.303);
        assert_eq!(t95(2.9), 4.303);
        assert_eq!(t95(45.0), 2.021);
        assert_eq!(t95(f64::INFINITY), 1.960);
    }
}
