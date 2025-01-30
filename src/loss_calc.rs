use argmin::{core::{CostFunction, Error, Executor, Gradient, State}, solver::{gradientdescent::SteepestDescent, linesearch::MoreThuenteLineSearch}};

#[derive(Clone)]
pub struct Bundle {
    ata_x: f64,
    ata_y: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    fee_rate: f64,
    fee_retention: f64,
}

impl Bundle {
    pub fn new(ata_x: f64, ata_y: f64, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, fee_rate: f64, fee_retention: f64) -> Self {
        Self {
            ata_x: ata_x,
            ata_y: ata_y,
            x1: x1,
            y1: y1,
            x2: x2,
            y2: y2,
            x3: x3,
            y3: y3,
            fee_rate: fee_rate,
            fee_retention: fee_retention,
        }
    }
    fn cpamm(x: f64, y: f64, x1: f64, r: f64, t: f64) -> (f64, f64, f64) {
        let k = x * y;
        let y1 = k / (x + x1 * (1f64 - r));
        return (y - y1, x + x1 * (1f64 - r * (1f64 - t)), y1);
    }

    fn cpamm_in(x: f64, y: f64, y1: f64, r: f64) -> f64 {
        let k = x * y;
        let x1 = k / (y - y1) - x;
        return x1 / (1f64 - r);
    }

    pub fn calc_swap_x(&self, x1: f64) -> (f64, f64, f64) {
        Self::cpamm(self.ata_x, self.ata_y, x1, self.fee_rate, self.fee_retention)
    }

    pub fn calc_swap_x_in(&self, y1: f64) -> f64 {
        Self::cpamm_in(self.ata_x, self.ata_y, y1, self.fee_rate)
    }

    fn calc_swap_y(&self, opt_result: &Vec<f64>, y1: f64) -> (f64, f64, f64) {
        Self::cpamm(self.ata_y * opt_result[1].exp(), self.ata_x * opt_result[0].exp(), y1, self.fee_rate, self.fee_retention)
    }

    fn loss(&self, px: f64, py: f64) -> f64 {
        let ix = self.ata_x * px.exp();
        let iy = self.ata_y * py.exp();
        let (y1, x, y) = Self::cpamm(ix, iy, self.x1, self.fee_rate, self.fee_retention);
        let (y2, x, y) = Self::cpamm(x, y, self.x2, self.fee_rate, self.fee_retention);
        let (x3, y, x) = Self::cpamm(y, x, self.y3, self.fee_rate, self.fee_retention);
        return (y1 / self.y1 - 1f64).powi(2) + (y2 / self.y2 - 1f64).powi(2) + (x3 / self.x3 - 1f64).powi(2);
    }

    pub fn update_initial_balances(&mut self, possible_fee_rates: &[&f64]) -> Result<(), Error> {
        let opt_result = possible_fee_rates.iter().fold(None, |acc, fee_rate| {
            let init_param = vec![0.0, 0.0];
            let linesearch = MoreThuenteLineSearch::new();
            let solver = SteepestDescent::new(linesearch);
            let res = Executor::new(self.clone(), solver)
                .configure(|state| state.param(init_param).max_iters(1000))
                .run();
            if let Ok(res) = res {
                let opt_result = res.state.get_best_param().unwrap();
                let loss = res.state.get_best_cost();
                match acc {
                    None => Some((*fee_rate, loss, opt_result.clone())),
                    Some((_, acc_loss, _)) => {
                        if loss < acc_loss {
                            Some((*fee_rate, loss, opt_result.clone()))
                        } else {
                            acc
                        }
                    }
                }
            } else {
                acc
            }
        });
        if let Some(opt_result) = opt_result {
            self.fee_rate = *opt_result.0;
            self.ata_x = self.ata_x * opt_result.2[0].exp();
            self.ata_y = self.ata_y * opt_result.2[1].exp();
        }
        Ok(())
    }

    pub fn user_losses(&self) -> (u64, u64) {
        let should_get = self.calc_swap_x(self.x2).0;
        let should_pay = self.calc_swap_x_in(self.y2);
        return ((should_get - self.y2) as u64, (self.x2 - should_pay) as u64);
    }
}

impl CostFunction for Bundle {
    type Param = Vec<f64>;
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, Error> {
        Ok(self.loss(p[0], p[1]))
    }
}

impl Gradient for Bundle {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;

    fn gradient(&self, p: &Self::Param) -> Result<Self::Gradient, Error> {
        let h = 1e-6;
        let mut grad = vec![0.0; 2];
        for i in 0..2 {
            let mut p1 = p.clone();
            let mut p2 = p.clone();
            p1[i] += h;
            p2[i] -= h;
            grad[i] = (self.loss(p1[0], p1[1]) - self.loss(p2[0], p2[1])) / (2.0 * h);
        }
        Ok(grad)
    }
}

pub fn main() {
    // inputs
    let ata_x = 763955577093.0;
    let ata_y = 35056259782319.0;
    let x1 = 93703641384.0;
    let y1 = 3821543694385.0;
    let x2 = 5000000000.0;
    let y2 = 180587421422.0;
    let x3 = 94164005402.0;
    let y3 = 3814610506929.0;
    let mut bundle = Bundle {
        ata_x: ata_x,
        ata_y: ata_y,
        x1: x1,
        y1: y1,
        x2: x2,
        y2: y2,
        x3: x3,
        y3: y3,
        fee_rate: 0.0025f64,
        fee_retention: 0.84f64,
    };
    bundle.update_initial_balances(&[&0.0025]).unwrap();

    // print result
    let should_get = bundle.calc_swap_x(bundle.x2).0;
    println!("should've gotten {}", should_get);
    println!("actual gotten {}", bundle.y2);
    println!("diff {} ({:.2}% loss)", should_get - bundle.y2, (should_get - bundle.y2) / bundle.y2 * 100f64);
    let should_pay = bundle.calc_swap_x_in(bundle.y2);
    println!("should've paid {}", should_pay);
    println!("actual paid {}", bundle.x2);
    println!("diff {} ({:.2}% loss)", bundle.x2 - should_pay, (bundle.x2 - should_pay) / bundle.x2 * 100f64);
}
