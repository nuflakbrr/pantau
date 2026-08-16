/// Calculates a system health score between 0 and 100
/// based on CPU load, memory pressure, disk free percentage, and temperature.
pub fn calculate_health_score(
    cpu_usage_pct: f64,
    mem_used_pct: f64,
    disk_used_pct: f64,
    temp_celsius: Option<f64>,
) -> u8 {
    let mut score = 100.0f64;

    // CPU deduction
    if cpu_usage_pct > 85.0 {
        score -= 25.0;
    } else if cpu_usage_pct > 65.0 {
        score -= 15.0;
    } else if cpu_usage_pct > 45.0 {
        score -= 5.0;
    }

    // Memory deduction
    if mem_used_pct > 90.0 {
        score -= 30.0;
    } else if mem_used_pct > 75.0 {
        score -= 15.0;
    } else if mem_used_pct > 60.0 {
        score -= 5.0;
    }

    // Disk deduction
    if disk_used_pct > 95.0 {
        score -= 25.0;
    } else if disk_used_pct > 85.0 {
        score -= 10.0;
    }

    // Temperature deduction (if available)
    if let Some(temp) = temp_celsius {
        if temp > 90.0 {
            score -= 20.0;
        } else if temp > 75.0 {
            score -= 10.0;
        }
    }

    score.clamp(0.0, 100.0).round() as u8
}
