mod context_tree;

use std::fs::File;

use halo2_base::{utils::ScalarField, Context};

use context_tree::ContextTree;

// TODO: Add something like `ContextTree` to upstream halo2-lib's Context
pub struct ContextWrapper<'ctx, F: ScalarField> {
    pub ctx: &'ctx mut Context<F>,
    tree: ContextTree,
}

impl<'ctx, F: ScalarField> ContextWrapper<'ctx, F> {
    pub fn new(ctx: &'ctx mut Context<F>) -> Self {
        Self {
            ctx,
            tree: ContextTree::new(),
        }
    }

    fn num_cells(&self) -> usize {
        self.ctx.advice.len()
    }

    pub fn push_context(&mut self, level: log::Level, ctx: &str) {
        self.tree.push(ctx, level, self.num_cells());
    }

    pub fn pop_context(&mut self) {
        self.tree.pop(self.num_cells());
    }

    // TODO: Remove min_delta if flamegraph is good.
    pub fn print_cell_counts(&self, min_delta: usize) {
        // Print cell counts for each context.
        self.tree
            .filter(self.num_cells(), min_delta)
            .print(self.num_cells());

        // TODO: Accumulate counts for each function and print aggregate counts.
        // // Print total count of each gate type.
        // debug!("Total counts:");
        // for gate in self.gates.iter().cloned() {
        //     let count = self
        //         .gate_instances
        //         .iter()
        //         .filter(|inst| inst.gate_ref == gate)
        //         .count();
        //     debug!("- {} instances of {}", count, gate.0.id());
        // }
    }

    pub fn write_cell_counts(&self, path: &str, min_delta: usize) {
        let mut file = File::create(path).expect("failed to create file");

        self.tree
            .filter(self.num_cells(), min_delta)
            .write(&mut file, self.num_cells());
    }

    pub fn write_cell_counts_flamegraph(&self, svg_path: &str, min_delta: usize) {
        let mut file = File::create(svg_path).expect("failed to create file");

        self.tree
            .filter(self.num_cells(), min_delta)
            .write_flamegraph(&mut file, self.num_cells());
    }
}
