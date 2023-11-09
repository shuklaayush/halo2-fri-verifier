use halo2_base::gates::{GateChip, RangeChip};
use halo2_base::gates::{GateInstructions, RangeInstructions};
use halo2_base::utils::ScalarField;
use halo2_base::virtual_region::lookups::LookupAnyManager;
use halo2_base::{AssignedValue, Context};
use plonky2::field::types::PrimeField64;
use std::marker::PhantomData;

use super::FieldChip;

const MAX_PHASE: usize = 3;

#[derive(Debug, Clone)]
pub struct Fp<F: ScalarField, F64: PrimeField64> {
    pub native: AssignedValue<F>,
    pub value: u64,

    _marker: PhantomData<F64>,
}

impl<F: ScalarField, F64: PrimeField64> Fp<F, F64> {
    pub fn new(native: AssignedValue<F>, value: u64) -> Self {
        Self {
            native,
            value,
            _marker: PhantomData,
        }
    }
}

// TODO: Reference and lifetimes? Should FpChip own RangeChip?
#[derive(Debug, Clone)]
pub struct FpChip<F: ScalarField, F64: PrimeField64> {
    pub range: RangeChip<F>, // TODO: Change to reference and add lifetime?
    _marker: PhantomData<F64>,
}

// TODO: Abstract away trait
impl<F: ScalarField, F64: PrimeField64> FpChip<F, F64> {
    pub fn new(lookup_bits: usize, lookup_manager: [LookupAnyManager<F, 1>; MAX_PHASE]) -> Self {
        Self {
            range: RangeChip::<F>::new(lookup_bits, lookup_manager),
            _marker: PhantomData,
        }
    }

    pub fn gate(&self) -> &GateChip<F> {
        self.range.gate()
    }

    pub fn range(&self) -> &RangeChip<F> {
        &self.range
    }
}

impl<F: ScalarField, F64: PrimeField64> FieldChip<F, F64, Fp<F, F64>> for FpChip<F, F64> {
    fn load_constant(&self, ctx: &mut Context<F>, a: F64) -> Fp<F, F64> {
        let a = a.to_canonical_u64();
        Fp::new(ctx.load_constant(F::from(a)), a)
    }

    fn load_constants<const N: usize>(
        &self,
        ctx: &mut Context<F>,
        a: &[F64; N],
    ) -> [Fp<F, F64>; N] {
        a.iter()
            .map(|a| self.load_constant(ctx, *a))
            .collect::<Vec<Fp<F, F64>>>() // TODO: There must be a better way
            .try_into()
            .unwrap()
    }

    fn load_witness(&self, ctx: &mut Context<F>, a: F64) -> Fp<F, F64> {
        let a = a.to_canonical_u64();
        Fp::new(ctx.load_witness(F::from(a)), a)
    }

    fn add(&self, ctx: &mut Context<F>, a: &Fp<F, F64>, b: &Fp<F, F64>) -> Fp<F, F64> {
        let one = self.load_constant(ctx, F64::ONE);
        self.mul_add(ctx, a, &one, b)
    }

    fn sub(&self, ctx: &mut Context<F>, a: &Fp<F, F64>, b: &Fp<F, F64>) -> Fp<F, F64> {
        let minus_one = self.load_constant(ctx, F64::ZERO - F64::ONE);
        self.mul_add(ctx, a, &minus_one, b)
    }

    // TODO: Add functions that don't reduce to chain operations and reduce at end
    fn mul(&self, ctx: &mut Context<F>, a: &Fp<F, F64>, b: &Fp<F, F64>) -> Fp<F, F64> {
        let zero = self.load_constant(ctx, F64::ZERO);
        self.mul_add(ctx, a, b, &zero)
    }

    fn mul_add(
        &self,
        ctx: &mut Context<F>,
        a: &Fp<F, F64>,
        b: &Fp<F, F64>,
        c: &Fp<F, F64>,
    ) -> Fp<F, F64> {
        // 1. Calculate hint
        let product: u128 = (a.value as u128) * (b.value as u128);
        let sum = product + (c.value as u128);
        let quotient = (sum / (F64::ORDER as u128)) as u64;
        let remainder = (sum % (F64::ORDER as u128)) as u64;

        // 2. Load witnesses from hint
        let quotient = self.load_witness(ctx, F64::from_canonical_u64(quotient));
        let remainder = self.load_witness(ctx, F64::from_canonical_u64(remainder));

        // 3. Constrain witnesses
        let gate = self.gate();
        let lhs = gate.mul_add(ctx, a.native, b.native, c.native);
        let p = ctx.load_constant(F::from(F64::ORDER)); // TODO: Cache
        let rhs = gate.mul_add(ctx, p, quotient.native, remainder.native);

        gate.is_equal(ctx, lhs, rhs);

        let range = self.range();
        range.is_less_than_safe(ctx, quotient.native, F64::ORDER);
        range.is_less_than_safe(ctx, remainder.native, F64::ORDER);

        // Return
        remainder
    }

    fn range_check(&self, ctx: &mut Context<F>, a: &Fp<F, F64>) {
        let range = self.range();
        let p = ctx.load_constant(F::from(F64::ORDER)); // TODO: Cache
        range.check_less_than(ctx, a.native, p, 64);
    }

    fn assert_equal(&self, ctx: &mut Context<F>, a: &Fp<F, F64>, b: &Fp<F, F64>) {
        ctx.constrain_equal(&a.native, &b.native);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo2_base::gates::circuit::builder::RangeCircuitBuilder;
    use halo2_proofs::dev::MockProver;
    use halo2curves::bn256::Fr;
    use plonky2::field::{goldilocks_field::GoldilocksField, types::Sample};
    use rand::rngs::StdRng;
    use rand_core::SeedableRng;

    #[test]
    fn test_fp_chip() {
        let mut rng = StdRng::seed_from_u64(0);

        let k = 16;
        let lookup_bits = 8;
        let unusable_rows = 9;

        let mut builder = RangeCircuitBuilder::default().use_k(k as usize);
        builder.set_lookup_bits(lookup_bits);

        let fp_chip =
            FpChip::<Fr, GoldilocksField>::new(lookup_bits, builder.lookup_manager().clone()); // TODO: Why clone?

        // TODO: What is builder.main(0)?
        let ctx = builder.main(0);

        for _ in 0..100 {
            let a = GoldilocksField::sample(&mut rng);
            let b = GoldilocksField::sample(&mut rng);

            let c1 = fp_chip.load_constant(ctx, a * b);

            let a_wire = fp_chip.load_constant(ctx, a);
            let b_wire = fp_chip.load_constant(ctx, b);
            let c2 = fp_chip.mul(ctx, &a_wire, &b_wire);

            fp_chip.assert_equal(ctx, &c1, &c2);
        }

        builder.calculate_params(Some(unusable_rows));

        MockProver::run(k, &builder, vec![])
            .unwrap()
            .assert_satisfied();
    }
}
