#[derive(Clone, Copy, Default)]
pub struct Hashrate(pub f32);

impl std::fmt::Display for Hashrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut hashrate = self.0;
        let mut unit = "";
        let units = vec!["k", "M", "G", "T", "P", "E", "Z", "Y"];
        for u in units.iter() {
            if hashrate > 1000.0 {
                hashrate /= 1000.0;
                unit = u;
            } else {
                break;
            }
        }
        write!(f, "{:.3} {}", hashrate, unit)
    }
}

impl core::iter::Sum for Hashrate {
    fn sum<I: Iterator<Item = Hashrate>>(iter: I) -> Hashrate {
        iter.fold(Hashrate(0.0), |a, b| Hashrate(a.0 + b.0))
    }
}
