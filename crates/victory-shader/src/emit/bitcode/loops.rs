//! Loops in the one shape the host's converter takes everywhere.
//!
//! Under Wine the host's D3D12 is vkd3d-proton, which turns our DXIL into
//! SPIR-V with dxil-spirv. Its control-flow code keeps one back edge per
//! block and one per loop header. Its main pass puts a module into that
//! shape first (`rewrite_multiple_back_edges`), but for a shader that reads
//! another lane of its wave (a "subgroup shuffle" there, as
//! `WaveReadLaneAt`) an earlier step, `flatten_subgroup_shuffles`, walks
//! the control flow as it came. `CFGStructurizer::visit` then keeps the
//! last back edge it met ("only support one back edge. DXIL seems to obey
//! this": an assertion, left out of the builds it ships in), the edges it
//! takes for forward ones hold a cycle, and
//! `LoopMergeTracer::trace_from_parent` calls itself without end. The
//! thread making the host pipeline runs out of stack and Wine ends that
//! thread without a word: the pipeline is never made, and every draw or
//! dispatch with it is dropped for the rest of the run.
//!
//! DXC's loops are in that shape; ours were not always:
//!
//! - a loop the lowering closes from two places has two latches;
//! - a block whose two successors were the empty latches of an inner and an
//!   outer loop jumped back to both headers once the writer had folded the
//!   two latches into it ([`Module::layout`], which no longer does).
//!
//! Five of the title's compute programs were lost that way (ray-traced
//! shadow traces on any host, prop instance culling as translated for a
//! host of 32-lane waves).
//!
//! [`Module::single_latches`] gives each such loop a latch of its own: the
//! blocks that jumped back to the header jump to the latch, the latch jumps
//! back, and a value the header joined from them is joined in the latch.
//! The module computes the same values on every path, and it is what the
//! converter's main pass would have made of the loop itself.
//! `VICTORY_SINGLE_LATCH=0` leaves the loops as the lowering made them.

use super::{Module, Op, Succs, Term, V, successors};

/// Whether loops get one latch each (`VICTORY_SINGLE_LATCH`, on unless 0).
/// Part of the translation: the shader cache keys on it.
pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SINGLE_LATCH").is_none_or(|v| v != "0"))
}

/// The edges that jump back, as (block, target): from a block to one that a
/// depth-first walk from the entry is still inside of, successors taken in
/// branch order. That is the walk the converter decides by.
fn back_edges(blocks: usize, succs: impl Fn(usize) -> Succs) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    if blocks == 0 {
        return edges;
    }
    let mut seen = vec![false; blocks];
    let mut inside = vec![false; blocks];
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    inside[0] = true;
    while let Some(top) = stack.last_mut() {
        let (b, k) = *top;
        top.1 += 1;
        match succs(b).get(k) {
            Some(&s) if inside[s] => edges.push((b, s)),
            Some(&s) if !seen[s] => {
                seen[s] = true;
                inside[s] = true;
                stack.push((s, 0));
            }
            Some(_) => {}
            None => {
                inside[b] = false;
                stack.pop();
            }
        }
    }
    edges
}

/// Per loop header, the blocks that jump back to it; per block, the headers
/// it jumps back to (each once, in the order found).
fn by_end(blocks: usize, edges: &[(usize, usize)]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut from = vec![Vec::new(); blocks];
    let mut to = vec![Vec::new(); blocks];
    for &(b, h) in edges {
        if !from[h].contains(&b) {
            from[h].push(b);
        }
        if !to[b].contains(&h) {
            to[b].push(h);
        }
    }
    (from, to)
}

impl Module {
    /// Gives a latch of its own to every loop that more than one block
    /// jumps back to, or that a block jumps back to which also jumps back
    /// to another loop. Returns how many loops got one.
    pub(crate) fn single_latches(&mut self) -> usize {
        let blocks = self.blocks.len();
        let edges = back_edges(blocks, |b| successors(self.blocks[b].term));
        let (from, to) = by_end(blocks, &edges);
        let headers: Vec<usize> = (0..blocks)
            .filter(|&h| from[h].len() > 1 || from[h].iter().any(|&b| to[b].len() > 1))
            .collect();
        for &h in &headers {
            let latch = self.new_block();
            self.terminate(latch, Term::Br(h));
            let sources = &from[h];
            let aim = |t: usize| if t == h { latch } else { t };
            for &b in sources {
                self.blocks[b].term = match self.blocks[b].term {
                    Some(Term::Br(t)) => Some(Term::Br(aim(t))),
                    Some(Term::CondBr(c, t, f)) => Some(Term::CondBr(c, aim(t), aim(f))),
                    other => other,
                };
            }
            // What the header joined from those blocks is joined in the
            // latch (one value from all of them needs no join), and the
            // header takes it from there.
            for p in self.blocks[h].phis.clone() {
                let Op::Phi(incoming) = &self.insts[p as usize].op else {
                    continue;
                };
                let (back, mut rest): (Vec<(V, usize)>, Vec<(V, usize)>) = incoming
                    .iter()
                    .copied()
                    .partition(|(_, b)| sources.contains(b));
                let Some(&(first, _)) = back.first() else {
                    continue;
                };
                let value = if back.iter().all(|&(v, _)| v == first) {
                    first
                } else {
                    let join = self.phi(latch, self.insts[p as usize].ty);
                    self.set_phi(join, back);
                    join
                };
                rest.push((value, latch));
                self.insts[p as usize].op = Op::Phi(rest);
            }
        }
        headers.len()
    }

    /// What is left of the two shapes in the module as it is written
    /// (after the writer's latch folding): the blocks with more than one
    /// back edge out and the loop headers with more than one in, by the
    /// position each is written at. Empty for a module the converter takes.
    #[cfg(test)]
    pub(crate) fn back_edge_faults(&self) -> Vec<String> {
        let (layout, fold) = self.layout();
        let mut at = vec![usize::MAX; self.blocks.len()];
        for (n, &b) in layout.iter().enumerate() {
            at[b] = n;
        }
        // (a branch to a folded latch is written as one to its header)
        let written = |n: usize| {
            let mut s = successors(self.blocks[layout[n]].term);
            for k in 0..s.len {
                let t = s.to[k];
                s.to[k] = at[fold[t].map_or(t, |(_, h)| h)];
            }
            s
        };
        let edges = back_edges(layout.len(), written);
        let (from, to) = by_end(layout.len(), &edges);
        let mut faults = Vec::new();
        for n in 0..layout.len() {
            if to[n].len() > 1 {
                faults.push(format!("block {n} jumps back to {:?}", to[n]));
            }
            if from[n].len() > 1 {
                faults.push(format!("header {n} is jumped back to from {:?}", from[n]));
            }
        }
        faults
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Attr, pred};
    use super::*;

    /// A module with a `main`, a function `f(i32) -> i32` with an effect
    /// (so its calls are written), and `n` more blocks after the entry.
    fn module(n: usize) -> (Module, V, Vec<usize>) {
        let mut m = Module::default();
        let i32t = m.int(32);
        let void = m.void();
        let fty = m.func_ty(i32t, vec![i32t]);
        let f = m.function("dx.op.test", fty, Some(Attr::NoUnwind), false);
        let main_ty = m.func_ty(void, vec![]);
        m.function("main", main_ty, None, true);
        let blocks = (0..=n).map(|_| m.new_block()).collect();
        (m, f, blocks)
    }

    /// The incoming values of a phi, by block.
    fn incoming(m: &Module, phi: V) -> Vec<(V, usize)> {
        let V::Inst(i) = phi else { panic!() };
        match &m.insts[i as usize].op {
            Op::Phi(inc) => inc.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// A loop closed from two places: the header's counter comes back
    /// stepped by one from the first and by two from the second.
    fn two_latches() -> (Module, V, [usize; 6], [V; 3]) {
        let (mut m, f, b) = module(5);
        let [entry, head, first, second, other, exit] = b[..] else {
            panic!()
        };
        let i32t = m.int(32);
        let (zero, one, two, nine) = (m.i32(0), m.i32(1), m.i32(2), m.i32(9));
        m.terminate(entry, Term::Br(head));
        let i = m.phi(head, i32t);
        let seen = m.call(head, f, vec![i]);
        let go = m.cmp(head, pred::IULT, seen, nine);
        m.terminate(head, Term::CondBr(go, first, exit));
        // (the latches are not empty: the writer folds neither)
        let a = m.bin(first, super::super::Bin::Add, i, one);
        let odd = m.cmp(first, pred::EQ, a, one);
        m.terminate(first, Term::CondBr(odd, head, second));
        let c = m.bin(second, super::super::Bin::Add, i, two);
        let even = m.cmp(second, pred::EQ, c, two);
        m.terminate(second, Term::CondBr(even, head, other));
        m.terminate(other, Term::Br(exit));
        m.terminate(exit, Term::Ret);
        m.set_phi(i, vec![(zero, entry), (a, first), (c, second)]);
        (
            m,
            i,
            [entry, head, first, second, other, exit],
            [zero, a, c],
        )
    }

    #[test]
    fn a_loop_closed_from_two_places_gets_one_latch() {
        let (mut m, i, [entry, head, first, second, ..], [zero, a, c]) = two_latches();
        assert_eq!(m.back_edge_faults().len(), 1, "{:?}", m.back_edge_faults());
        assert_eq!(m.single_latches(), 1);
        assert_eq!(m.back_edge_faults(), Vec::<String>::new());
        // The header takes its start from the entry and the rest from the
        // latch, which joins what the two blocks stepped the counter to.
        let latch = m.blocks.len() - 1;
        let at_head = incoming(&m, i);
        assert_eq!(at_head.len(), 2);
        assert_eq!(at_head[0], (zero, entry));
        let (join, from) = at_head[1];
        assert_eq!(from, latch);
        assert_eq!(incoming(&m, join), vec![(a, first), (c, second)]);
        assert!(matches!(m.blocks[latch].term, Some(Term::Br(h)) if h == head));
        assert!(
            matches!(m.blocks[first].term, Some(Term::CondBr(_, t, f)) if t == latch && f == second)
        );
        // A second run finds nothing to do.
        assert_eq!(m.single_latches(), 0);
    }

    #[test]
    fn one_value_from_every_latch_needs_no_join() {
        let (mut m, i, _, [zero, a, _]) = two_latches();
        let [(_, entry), (_, first), (_, second)] = incoming(&m, i)[..] else {
            panic!()
        };
        m.set_phi(i, vec![(zero, entry), (a, first), (a, second)]);
        assert_eq!(m.single_latches(), 1);
        let latch = m.blocks.len() - 1;
        assert_eq!(incoming(&m, i), vec![(zero, entry), (a, latch)]);
        assert!(m.blocks[latch].phis.is_empty());
    }

    /// An inner loop in an outer one, and a block whose two successors are
    /// the empty latches of the two: the shape the writer's folding turned
    /// into one block that jumps back to both headers.
    fn two_loops_one_tail() -> (Module, [usize; 7]) {
        let (mut m, f, b) = module(6);
        let [entry, outer, inner, tail, inner_latch, outer_latch, exit] = b[..] else {
            panic!()
        };
        let (one, nine) = (m.i32(1), m.i32(9));
        m.terminate(entry, Term::Br(outer));
        m.terminate(outer, Term::Br(inner));
        let x = m.call(inner, f, vec![one]);
        let go = m.cmp(inner, pred::IULT, x, nine);
        m.terminate(inner, Term::CondBr(go, tail, exit));
        let y = m.call(tail, f, vec![x]);
        let again = m.cmp(tail, pred::EQ, y, one);
        m.terminate(tail, Term::CondBr(again, inner_latch, outer_latch));
        m.terminate(inner_latch, Term::Br(inner));
        m.terminate(outer_latch, Term::Br(outer));
        m.terminate(exit, Term::Ret);
        (
            m,
            [entry, outer, inner, tail, inner_latch, outer_latch, exit],
        )
    }

    #[test]
    fn only_one_of_a_block_s_two_latches_is_folded_into_it() {
        let (m, [_, outer, inner, tail, inner_latch, outer_latch, _]) = two_loops_one_tail();
        assert_eq!(m.back_edge_faults(), Vec::<String>::new());
        let (layout, fold) = m.layout();
        // The inner loop's latch is folded: the block that leaves the inner
        // loop is its latch, as in DXC's loops. The outer loop keeps its own.
        assert_eq!(fold[inner_latch], Some((tail, inner)));
        assert_eq!(fold[outer_latch], None);
        assert!(layout.contains(&outer_latch) && !layout.contains(&inner_latch));
        let _ = outer;
    }

    #[test]
    fn a_block_that_jumps_back_to_two_loops_gets_them_a_latch_each() {
        let (mut m, [_, outer, inner, tail, inner_latch, outer_latch, _]) = two_loops_one_tail();
        // The lowering's own form of it: no latch blocks at all.
        let Some(Term::CondBr(again, ..)) = m.blocks[tail].term else {
            panic!()
        };
        m.terminate(tail, Term::CondBr(again, inner, outer));
        assert_eq!(m.back_edge_faults().len(), 1, "{:?}", m.back_edge_faults());
        assert_eq!(m.single_latches(), 2);
        assert_eq!(m.back_edge_faults(), Vec::<String>::new());
        let _ = (inner_latch, outer_latch);
    }

    #[test]
    fn a_loop_with_one_latch_is_left_alone() {
        let (mut m, f, b) = module(3);
        let [entry, head, body, exit] = b[..] else {
            panic!()
        };
        let i32t = m.int(32);
        let (zero, one, nine) = (m.i32(0), m.i32(1), m.i32(9));
        m.terminate(entry, Term::Br(head));
        let i = m.phi(head, i32t);
        m.terminate(head, Term::Br(body));
        let next = m.bin(body, super::super::Bin::Add, i, one);
        let seen = m.call(body, f, vec![next]);
        let go = m.cmp(body, pred::IULT, seen, nine);
        m.terminate(body, Term::CondBr(go, head, exit));
        m.terminate(exit, Term::Ret);
        m.set_phi(i, vec![(zero, entry), (next, body)]);
        let before = m.write();
        assert_eq!(m.single_latches(), 0);
        assert_eq!(m.back_edge_faults(), Vec::<String>::new());
        assert_eq!(m.write(), before);
    }
}
