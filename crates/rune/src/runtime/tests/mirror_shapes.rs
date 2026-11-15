//! `El::mirror_in_rtl` mirrors a shape's emitted geometry about its box, in the flow Rune
//! resolves, including inside a reused memo.
use super::*;
use crate::{Color, Direction, LineCap, Path, Shape, canvas, memo, text};
use std::cell::Cell;
use std::rc::Rc;

const W: f32 = 200.0;
const H: f32 = 100.0;
const BOX: f32 = 100.0;
const TURN: f32 = 0.4;
const SCALE: f32 = 2.0;

/// An asymmetric closed curve with both a cubic and a quadratic run.
fn curve() -> Path {
    Path::new()
        .move_to([0.0, 50.0])
        .cubic_to([12.0, 0.0], [88.0, 18.0], [100.0, 50.0])
        .quad_to([42.0, 100.0], [0.0, 50.0])
        .close()
}

fn shape(view_box: bool) -> Shape {
    let path = curve();
    let shape = Shape::new()
        .fill_path(&path, Color::WHITE)
        .stroke_path(&path, 2.5, LineCap::Round, Color::WHITE);
    if view_box { shape.view_box(100.0, 100.0) } else { shape }
}

#[derive(Clone, Debug)]
struct Piece {
    kind: f32,
    points: [[f32; 2]; 3],
    fill: [f32; 4],
    clip: [f32; 4],
    param: f32,
}

fn pieces<S>(rt: &Runtime<S>) -> Vec<Piece> {
    rt.instances
        .iter()
        .filter_map(|q| {
            let kind = q.params[1];
            let points = if kind == KIND_SEGMENT {
                [[q.radii[0], q.radii[1]], [q.radii[2], q.radii[3]], [q.radii[2], q.radii[3]]]
            } else if kind == KIND_TRIANGLE {
                [[q.radii[0], q.radii[1]], [q.radii[2], q.radii[3]], [q.uv[0], q.uv[1]]]
            } else {
                return None;
            };
            Some(Piece { kind, points, fill: q.fill, clip: q.clip, param: q.params[0] })
        })
        .collect()
}

fn box_centre<S>(rt: &Runtime<S>, key: &str) -> f32 {
    let n = rt.nodes.iter().find(|n| n.key.as_deref() == Some(key)).expect("the shape is mounted");
    (n.target[0] + n.target[2]) * 0.5
}

fn turn<S>(rt: &Runtime<S>) -> f32 {
    rt.instances.iter().find(|q| q.params[1] == crate::render::KIND_LAYER).expect("the turned element draws a layer").border[1]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// `on` draws the curve flipped about its box against `off`: the same strokes, fills, clips and
/// widths, with every x the other side of its own box's centre.
fn assert_flipped(off: &[Piece], off_centre: f32, on: &[Piece], on_centre: f32) {
    assert!(!off.is_empty(), "the shape emits geometry");
    assert_eq!(off.len(), on.len(), "a mirror draws the same pieces");
    for (a, b) in off.iter().zip(on) {
        assert_eq!((a.kind, a.param), (b.kind, b.param), "same kind and stroke width or edge mask");
        assert_eq!((a.fill, a.clip), (b.fill, b.clip), "same paint and clip");
        for (p, q) in a.points.iter().zip(&b.points) {
            assert!(close(q[0] - on_centre, off_centre - p[0]), "x flips: {q:?} from {p:?}");
            assert!(close(q[1], p[1]), "y stays: {q:?} from {p:?}");
        }
    }
}

fn assert_mirrored(off: &[Piece], on: &[Piece], centre: f32) {
    assert_flipped(off, centre, on, centre);
}

/// `on` draws exactly what `off` does.
fn assert_same(off: &[Piece], on: &[Piece]) {
    assert_eq!(off.len(), on.len());
    for (a, b) in off.iter().zip(on) {
        assert_eq!((a.kind, a.param, a.fill, a.clip), (b.kind, b.param, b.fill, b.clip));
        assert_eq!(a.points, b.points);
    }
}

#[derive(Clone, Copy)]
enum Case {
    Set(Direction),
    Island,
    Auto(&'static str),
}

struct State {
    case: Case,
    view_box: bool,
    mirror: bool,
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let curve = canvas(shape(s.view_box))
        .key("shape")
        .size(BOX, BOX)
        .rotate(TURN)
        .rotate_motion(rune_motion::Curve::Instant)
        .scale(SCALE)
        .scale_motion(rune_motion::Curve::Instant);
    let curve = if s.mirror { curve.mirror_in_rtl() } else { curve };
    let root = col().size(W, H);
    match s.case {
        Case::Set(dir) => root.dir(dir).child(curve),
        Case::Island => root
            .dir(Direction::Rtl)
            .child(col().key("island").size(BOX, BOX).dir(Direction::Ltr).child(curve)),
        Case::Auto(word) => root.dir(Direction::Auto).child(text(word).key("word")).child(curve),
    }
}

/// Runs one case with and without the flag, in the same resolved layout, and checks the
/// difference is the mirror of the drawing and of its turn.
fn check(case: Case, view_box: bool, mirrored: bool) {
    let run = |mirror| {
        let mut rt = Runtime::new(State { case, view_box, mirror }, view, (W, H), Env::new(true, 1.0));
        rt.frame(0.0);
        rt.frame(1.0 / 120.0);
        rt
    };
    let (off, on) = (run(false), run(true));
    let (a, b) = (pieces(&off), pieces(&on));
    if mirrored {
        assert_mirrored(&a, &b, box_centre(&on, "shape"));
        assert!(close(turn(&on), -TURN), "a mirrored drawing turns the other way");
    } else {
        assert_same(&a, &b);
        assert!(close(turn(&on), TURN));
    }
    // The element's scale is in the emitted geometry, not only its layer.
    let xs = b.iter().flat_map(|p| p.points.iter().map(|p| p[0])).fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
    assert!(close(xs.1 - xs.0, BOX * SCALE), "the curve spans its scaled box: {xs:?}");
}

#[test]
fn shapes_mirror_about_their_box_in_rtl_but_not_inside_an_ltr_island() {
    for view_box in [false, true] {
        check(Case::Set(Direction::Rtl), view_box, true);
        check(Case::Set(Direction::Ltr), view_box, false);
        check(Case::Island, view_box, false);
    }
}

#[test]
fn an_auto_container_mirrors_its_shape_by_its_first_strong_text() {
    for view_box in [false, true] {
        check(Case::Auto("مرحبا"), view_box, true);
        check(Case::Auto("hello"), view_box, false);
    }
}

struct Progress {
    mirror: bool,
    trim: [f32; 2],
}

fn progress_view(s: &Progress, _: &mut Cx<Progress>) -> El {
    let curve = canvas(shape(false).trimmed(s.trim[0], s.trim[1])).key("shape").size(BOX, BOX);
    let curve = if s.mirror { curve.mirror_in_rtl() } else { curve };
    col().size(W, H).dir(Direction::Rtl).child(curve)
}

#[test]
fn a_trimmed_shape_mirrors_only_the_stretch_it_draws() {
    let run = |trim, mirror| {
        let mut rt = Runtime::new(Progress { mirror, trim }, progress_view, (W, H), Env::new(true, 1.0));
        rt.frame(0.0);
        rt
    };
    let whole = pieces(&run([0.0, 1.0], false)).len();
    for trim in [[0.0, 0.55], [0.2, 0.7]] {
        let (off, on) = (run(trim, false), run(trim, true));
        let (a, b) = (pieces(&off), pieces(&on));
        assert!(b.len() > 0 && b.len() < whole, "{trim:?} draws part of the whole: {} of {whole}", b.len());
        assert_mirrored(&a, &b, box_centre(&on, "shape"));
    }
}

struct MemoState {
    dir: Direction,
    builds: Rc<Cell<u32>>,
}

fn memo_view(s: &MemoState, _: &mut Cx<MemoState>) -> El {
    let builds = s.builds.clone();
    col().size(W, H).dir(s.dir).child(memo("curve", 0, move || {
        builds.set(builds.get() + 1);
        canvas(shape(false)).key("shape").size(BOX, BOX).mirror_in_rtl()
    }))
}

#[test]
fn a_memo_reused_across_a_direction_change_mirrors_without_building_again() {
    let builds = Rc::new(Cell::new(0));
    let mut rt = Runtime::new(MemoState { dir: Direction::Rtl, builds: builds.clone() }, memo_view, (W, H), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_eq!(builds.get(), 1, "the memo builds once");
    let (rtl, centre) = (pieces(&rt), box_centre(&rt, "curve"));

    rt.update(|s, tx| { tx.instant(); s.dir = Direction::Ltr });
    rt.frame(1.0 / 120.0);
    assert_eq!(builds.get(), 1, "the closure doesn't run again");
    let ltr = pieces(&rt);
    assert_flipped(&ltr, box_centre(&rt, "curve"), &rtl, centre);
}
