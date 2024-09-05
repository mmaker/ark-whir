use super::proof::SumcheckPolynomial;
use crate::poly_utils::{coeffs::CoefficientList, evals::EvaluationsList, MultilinearPoint};
use ark_ff::Field;
#[cfg(feature = "parallel")]
use rayon::join;

pub struct SumcheckSingle<F> {
    // The evaluation of p
    evaluation_of_p: EvaluationsList<F>,
    evaluation_of_equality: EvaluationsList<F>,
    num_variables: usize,
    sum: F,
}

impl<F> SumcheckSingle<F>
where
    F: Field,
{
    // Get the coefficient of polynomial p and a list of points
    // and initialises the table of the initial polynomial
    // v(X_1, ..., X_n) = p(X_1, ... X_n) * (epsilon_1 eq_z_1(X) + epsilon_2 eq_z_2(X) ...)
    pub fn new(
        coeffs: CoefficientList<F>,
        points: &[MultilinearPoint<F>],
        combination_randomness: &[F],
        evaluations: &[F],
    ) -> Self {
        assert_eq!(points.len(), combination_randomness.len());
        assert_eq!(points.len(), evaluations.len());
        let num_variables = coeffs.num_variables();

        let mut prover = SumcheckSingle {
            evaluation_of_p: coeffs.into(),
            evaluation_of_equality: EvaluationsList::new(vec![F::ZERO; 1 << num_variables]),
            num_variables,
            sum: F::ZERO,
        };

        prover.add_new_equality(points, combination_randomness, evaluations);
        prover
    }

    pub fn compute_sumcheck_polynomial(&self) -> SumcheckPolynomial<F> {
        let two = F::ONE + F::ONE; // Enlightening (see Whitehead & Russell (1910) Thm. ✱54.43)
        assert!(self.num_variables >= 1);

        let prefix_len = 1 << (self.num_variables - 1);

        // Compute coefficients of the quadratic result polynomial
        let mut coeff_0 = F::ZERO;
        let mut coeff_2 = F::ZERO;

        for beta_prefix in 0..prefix_len {
            let eval_of_p_0 = self.evaluation_of_p[2 * beta_prefix];
            let eval_of_p_1 = self.evaluation_of_p[2 * beta_prefix + 1];

            // Coefficients of the linear `evaluation_of_p` polynomial
            let p_0 = eval_of_p_0;
            let p_1 = eval_of_p_1 - eval_of_p_0;

            let eval_of_eq_0 = self.evaluation_of_equality[2 * beta_prefix];
            let eval_of_eq_1 = self.evaluation_of_equality[2 * beta_prefix + 1];

            // Coefficients of the linear `evaluation_of_equality` polynomial
            let w_0 = eval_of_eq_0;
            let w_1 = eval_of_eq_1 - eval_of_eq_0;

            // Now we need to add the contribution of p(x) * w(x)
            coeff_0 += p_0 * w_0;
            coeff_2 += p_1 * w_1;
        }

        // Use the fact that self.sum = p(0) + p(1) = 2 * coeff_0 + coeff_1 + coeff_2
        let coeff_1 = self.sum - coeff_0 - coeff_0 - coeff_2;

        // Evaluate the quadratic polynomial at 0, 1, 2
        let eval_0 = coeff_0;
        let eval_1 = coeff_0 + coeff_1 + coeff_2;
        let eval_2 = coeff_0 + two * coeff_1 + two * two * coeff_2;

        SumcheckPolynomial::new(vec![eval_0, eval_1, eval_2], 1)
    }

    // Evaluate the eq function on for a given point on the hypercube, and add
    // the result multiplied by the scalar to the output.
    #[cfg(not(feature = "parallel"))]
    fn eval_eq(eval: &[F], out: &mut [F], scalar: F) {
        debug_assert_eq!(out.len(), 1 << eval.len());
        if let Some((&x, tail)) = eval.split_first() {
            let (low, high) = out.split_at_mut(out.len() / 2);
            let s1 = scalar * x;
            let s0 = scalar - s1;
            Self::eval_eq(tail, low, s0);
            Self::eval_eq(tail, high, s1);
        } else {
            out[0] += scalar;
        }
    }

    // Evaluate the eq function on a given point on the hypercube, and add
    // the result multiplied by the scalar to the output.
    #[cfg(feature = "parallel")]
    fn eval_eq(eval: &[F], out: &mut [F], scalar: F) {
        const PARALLEL_THRESHOLD: usize = 10;
        debug_assert_eq!(out.len(), 1 << eval.len());
        if let Some((&x, tail)) = eval.split_first() {
            let (low, high) = out.split_at_mut(out.len() / 2);
            // Update scalars using a single mul. Note that this causes a data dependency,
            // so for small fields it might be better to use two muls.
            // This data dependency should go away once we implement parallel point evaluation.
            let s1 = scalar * x;
            let s0 = scalar - s1;
            if tail.len() > PARALLEL_THRESHOLD {
                join(
                    || Self::eval_eq(tail, low, s0),
                    || Self::eval_eq(tail, high, s1),
                );
            } else {
                Self::eval_eq(tail, low, s0);
                Self::eval_eq(tail, high, s1);
            }
        } else {
            out[0] += scalar;
        }
    }

    pub fn add_new_equality(
        &mut self,
        points: &[MultilinearPoint<F>],
        combination_randomness: &[F],
        evaluations: &[F],
    ) {
        assert_eq!(combination_randomness.len(), points.len());
        assert_eq!(combination_randomness.len(), evaluations.len());
        for (point, rand) in points.iter().zip(combination_randomness) {
            // TODO: We might want to do all points simultaneously so we
            // do only a single pass over the data.
            Self::eval_eq(&point.0, self.evaluation_of_equality.evals_mut(), *rand);
        }

        // Update the sum
        for (rand, eval) in combination_randomness.iter().zip(evaluations.iter()) {
            self.sum += *rand * eval;
        }

        // Check sum invariant
        debug_assert_eq!(
            self.sum,
            self.evaluation_of_p
                .evals()
                .iter()
                .zip(self.evaluation_of_equality.evals().iter())
                .map(|(p, eq)| *p * eq)
                .sum()
        );
    }

    // When the folding randomness arrives, compress the table accordingly (adding the new points)
    pub fn compress(
        &mut self,
        combination_randomness: F, // Scale the initial point
        folding_randomness: &MultilinearPoint<F>,
        sumcheck_poly: &SumcheckPolynomial<F>,
    ) {
        assert_eq!(folding_randomness.n_variables(), 1);
        assert!(self.num_variables >= 1);

        let randomness = folding_randomness.0[0];
        let randomness_bar = F::ONE - randomness;

        let prefix_len = 1 << (self.num_variables - 1);
        let mut evaluations_of_p = Vec::with_capacity(prefix_len);
        let mut evaluations_of_eq = Vec::with_capacity(prefix_len);

        // Compress the table
        for beta_prefix in 0..prefix_len {
            let eval_of_p_0 = self.evaluation_of_p[2 * beta_prefix];
            let eval_of_p_1 = self.evaluation_of_p[2 * beta_prefix + 1];
            let eval_of_p = eval_of_p_0 * randomness_bar + eval_of_p_1 * randomness;

            let eval_of_eq_0 = self.evaluation_of_equality[2 * beta_prefix];
            let eval_of_eq_1 = self.evaluation_of_equality[2 * beta_prefix + 1];
            let eval_of_eq = eval_of_eq_0 * randomness_bar + eval_of_eq_1 * randomness;

            evaluations_of_p.push(eval_of_p);
            evaluations_of_eq.push(combination_randomness * eval_of_eq);
        }

        // Update
        self.num_variables -= 1;
        self.evaluation_of_p = EvaluationsList::new(evaluations_of_p);
        self.evaluation_of_equality = EvaluationsList::new(evaluations_of_eq);
        self.sum = combination_randomness * sumcheck_poly.evaluate_at_point(folding_randomness);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        crypto::fields::Field64,
        poly_utils::{coeffs::CoefficientList, MultilinearPoint},
    };

    use super::SumcheckSingle;

    type F = Field64;

    #[test]
    fn test_sumcheck_folding_factor_1() {
        let eval_point = MultilinearPoint(vec![F::from(10), F::from(11)]);
        let polynomial =
            CoefficientList::new(vec![F::from(1), F::from(5), F::from(10), F::from(14)]);

        let claimed_value = polynomial.evaluate(&eval_point);

        let eval = polynomial.evaluate(&eval_point);
        let mut prover = SumcheckSingle::new(polynomial, &[eval_point], &[F::from(1)], &[eval]);

        let poly_1 = prover.compute_sumcheck_polynomial();

        // First, check that is sums to the right value over the hypercube
        assert_eq!(poly_1.sum_over_hypercube(), claimed_value);

        let combination_randomness = F::from(100101);
        let folding_randomness = MultilinearPoint(vec![F::from(4999)]);

        prover.compress(combination_randomness, &folding_randomness, &poly_1);

        let poly_2 = prover.compute_sumcheck_polynomial();

        assert_eq!(
            poly_2.sum_over_hypercube(),
            combination_randomness * poly_1.evaluate_at_point(&folding_randomness)
        );
    }
}

#[test]
fn test_eval_eq() {
    use crate::crypto::fields::Field64 as F;
    use crate::poly_utils::sequential_lag_poly::LagrangePolynomialIterator;
    use ark_ff::AdditiveGroup;

    let eval = vec![F::from(3), F::from(5)];
    let mut out = vec![F::ZERO; 4];
    SumcheckSingle::eval_eq(&eval, &mut out, F::ONE);
    dbg!(&out);

    let point = MultilinearPoint(eval.clone());
    let mut expected = vec![F::ZERO; 4];
    for (prefix, lag) in LagrangePolynomialIterator::new(&point) {
        expected[prefix.0] = lag;
    }
    dbg!(&expected);

    assert_eq!(&out, &expected);
}
