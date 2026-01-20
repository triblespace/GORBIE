#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "../.." }
//! egui = "0.33"
//! ```

use egui::text::LayoutJob;
use egui::RichText;
use egui::TextStyle;
use std::ops::Range;
use GORBIE::cards::{with_padding, DEFAULT_CARD_PADDING};
use GORBIE::prelude::*;

#[derive(Default)]
struct QuizState {
    sum_result: Option<i32>,
    product_result: Option<i32>,
}

struct ExpressionState {
    input: String,
    step: usize,
}

impl Default for ExpressionState {
    fn default() -> Self {
        Self {
            input: "(3 + 2) * 2".to_string(),
            step: 0,
        }
    }
}

struct PairState {
    a: i32,
    b: i32,
}

#[derive(Clone)]
enum ExprKind {
    Num(i64),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

#[derive(Clone)]
struct Expr {
    kind: ExprKind,
}

impl Expr {
    fn num(value: i64) -> Self {
        Self {
            kind: ExprKind::Num(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathStep {
    Unary,
    Left,
    Right,
}

struct Step {
    expr: Expr,
    highlight: Option<Vec<PathStep>>,
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse_expression(&mut self) -> Result<Expr, String> {
        let expr = self.parse_sum()?;
        self.skip_ws();
        if self.pos < self.input.len() {
            return Err(format!("Unexpected input at position {}", self.pos + 1));
        }
        Ok(expr)
    }

    fn parse_sum(&mut self) -> Result<Expr, String> {
        let mut node = self.parse_product()?;
        loop {
            self.skip_ws();
            if self.consume(b'+') {
                let right = self.parse_product()?;
                node = Expr {
                    kind: ExprKind::Add(Box::new(node), Box::new(right)),
                };
            } else if self.consume(b'-') {
                let right = self.parse_product()?;
                node = Expr {
                    kind: ExprKind::Sub(Box::new(node), Box::new(right)),
                };
            } else {
                break;
            }
        }
        Ok(node)
    }

    fn parse_product(&mut self) -> Result<Expr, String> {
        let mut node = self.parse_factor()?;
        loop {
            self.skip_ws();
            if self.consume(b'*') {
                let right = self.parse_factor()?;
                node = Expr {
                    kind: ExprKind::Mul(Box::new(node), Box::new(right)),
                };
            } else {
                break;
            }
        }
        Ok(node)
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.consume(b'-') {
            let inner = self.parse_factor()?;
            return Ok(Expr {
                kind: ExprKind::Neg(Box::new(inner)),
            });
        }
        if self.consume(b'(') {
            let inner = self.parse_sum()?;
            self.skip_ws();
            if !self.consume(b')') {
                return Err(format!("Expected ')' at position {}", self.pos + 1));
            }
            return Ok(inner);
        }
        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let start = self.pos;
        let mut value: i64 = 0;
        while let Some(byte) = self.peek() {
            if !byte.is_ascii_digit() {
                break;
            }
            self.pos += 1;
            let digit = (byte - b'0') as i64;
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| "Number too large".to_string())?;
        }
        if self.pos == start {
            return Err(format!("Expected a number at position {}", self.pos + 1));
        }
        Ok(Expr::num(value))
    }

    fn skip_ws(&mut self) {
        while let Some(byte) = self.peek() {
            if !byte.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

fn parse_expression(input: &str) -> Result<Expr, String> {
    let mut parser = Parser::new(input);
    parser.parse_expression()
}

fn as_num(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::Num(value) => Some(value),
        _ => None,
    }
}

fn is_reducible(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Num(_) => false,
        ExprKind::Neg(inner) => as_num(inner).is_some(),
        ExprKind::Add(left, right)
        | ExprKind::Sub(left, right)
        | ExprKind::Mul(left, right) => as_num(left).is_some() && as_num(right).is_some(),
    }
}

fn eval_reducible(expr: &Expr) -> Result<i64, String> {
    match &expr.kind {
        ExprKind::Num(value) => Ok(*value),
        ExprKind::Neg(inner) => {
            let value = as_num(inner).ok_or_else(|| "Expected a number".to_string())?;
            value.checked_neg().ok_or_else(|| "Overflow".to_string())
        }
        ExprKind::Add(left, right) => {
            let left = as_num(left).ok_or_else(|| "Expected a number".to_string())?;
            let right = as_num(right).ok_or_else(|| "Expected a number".to_string())?;
            left.checked_add(right).ok_or_else(|| "Overflow".to_string())
        }
        ExprKind::Sub(left, right) => {
            let left = as_num(left).ok_or_else(|| "Expected a number".to_string())?;
            let right = as_num(right).ok_or_else(|| "Expected a number".to_string())?;
            left.checked_sub(right).ok_or_else(|| "Overflow".to_string())
        }
        ExprKind::Mul(left, right) => {
            let left = as_num(left).ok_or_else(|| "Expected a number".to_string())?;
            let right = as_num(right).ok_or_else(|| "Expected a number".to_string())?;
            left.checked_mul(right).ok_or_else(|| "Overflow".to_string())
        }
    }
}

fn find_reducible(expr: &Expr) -> Option<Vec<PathStep>> {
    match &expr.kind {
        ExprKind::Num(_) => None,
        ExprKind::Neg(inner) => find_reducible(inner)
            .map(|mut path| {
                path.insert(0, PathStep::Unary);
                path
            })
            .or_else(|| if is_reducible(expr) { Some(Vec::new()) } else { None }),
        ExprKind::Add(left, right)
        | ExprKind::Sub(left, right)
        | ExprKind::Mul(left, right) => find_reducible(left)
            .map(|mut path| {
                path.insert(0, PathStep::Left);
                path
            })
            .or_else(|| {
                find_reducible(right).map(|mut path| {
                    path.insert(0, PathStep::Right);
                    path
                })
            })
            .or_else(|| if is_reducible(expr) { Some(Vec::new()) } else { None }),
    }
}

fn reduce_at(expr: Expr, path: &[PathStep]) -> Result<Expr, String> {
    if path.is_empty() {
        return Ok(Expr::num(eval_reducible(&expr)?));
    }

    let (head, tail) = path.split_first().ok_or("Invalid path")?;
    match (head, expr.kind) {
        (PathStep::Unary, ExprKind::Neg(inner)) => Ok(Expr {
            kind: ExprKind::Neg(Box::new(reduce_at(*inner, tail)?)),
        }),
        (PathStep::Left, ExprKind::Add(left, right)) => Ok(Expr {
            kind: ExprKind::Add(Box::new(reduce_at(*left, tail)?), right),
        }),
        (PathStep::Right, ExprKind::Add(left, right)) => Ok(Expr {
            kind: ExprKind::Add(left, Box::new(reduce_at(*right, tail)?)),
        }),
        (PathStep::Left, ExprKind::Sub(left, right)) => Ok(Expr {
            kind: ExprKind::Sub(Box::new(reduce_at(*left, tail)?), right),
        }),
        (PathStep::Right, ExprKind::Sub(left, right)) => Ok(Expr {
            kind: ExprKind::Sub(left, Box::new(reduce_at(*right, tail)?)),
        }),
        (PathStep::Left, ExprKind::Mul(left, right)) => Ok(Expr {
            kind: ExprKind::Mul(Box::new(reduce_at(*left, tail)?), right),
        }),
        (PathStep::Right, ExprKind::Mul(left, right)) => Ok(Expr {
            kind: ExprKind::Mul(left, Box::new(reduce_at(*right, tail)?)),
        }),
        _ => Err("Invalid reduction path".to_string()),
    }
}

fn build_steps(expr: Expr) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    let mut current = expr;
    loop {
        if let Some(path) = find_reducible(&current) {
            steps.push(Step {
                expr: current.clone(),
                highlight: Some(path.clone()),
            });
            current = reduce_at(current, &path)?;
        } else {
            steps.push(Step {
                expr: current.clone(),
                highlight: None,
            });
            break;
        }
    }
    Ok(steps)
}

fn render_expr_with_highlight(expr: &Expr, highlight: Option<&[PathStep]>) -> (String, Vec<Range<usize>>) {
    let mut text = String::new();
    let mut highlight_range = None;
    let highlight_enabled = highlight.is_some();
    render_expr(expr, highlight.unwrap_or(&[]), highlight_enabled, &mut text, &mut highlight_range);
    let ranges = highlight_range.into_iter().collect();
    (text, ranges)
}

fn render_expr(
    expr: &Expr,
    highlight_path: &[PathStep],
    highlight_enabled: bool,
    out: &mut String,
    highlight_range: &mut Option<Range<usize>>,
) {
    let start = out.len();
    match &expr.kind {
        ExprKind::Num(value) => {
            out.push_str(&value.to_string());
        }
        ExprKind::Neg(inner) => {
            out.push_str("(-");
            let (child_path, child_highlight): (&[PathStep], bool) =
                match highlight_path.split_first() {
                    Some((PathStep::Unary, rest)) => (rest, highlight_enabled),
                    _ => (&[], false),
                };
            render_expr(inner, child_path, child_highlight, out, highlight_range);
            out.push(')');
        }
        ExprKind::Add(left, right) => {
            out.push('(');
            let (left_path, left_highlight, right_path, right_highlight): (
                &[PathStep],
                bool,
                &[PathStep],
                bool,
            ) = match highlight_path.split_first() {
                Some((PathStep::Left, rest)) => (rest, highlight_enabled, &[], false),
                Some((PathStep::Right, rest)) => (&[], false, rest, highlight_enabled),
                _ => (&[], false, &[], false),
            };
            render_expr(left, left_path, left_highlight, out, highlight_range);
            out.push_str(" + ");
            render_expr(right, right_path, right_highlight, out, highlight_range);
            out.push(')');
        }
        ExprKind::Sub(left, right) => {
            out.push('(');
            let (left_path, left_highlight, right_path, right_highlight): (
                &[PathStep],
                bool,
                &[PathStep],
                bool,
            ) = match highlight_path.split_first() {
                Some((PathStep::Left, rest)) => (rest, highlight_enabled, &[], false),
                Some((PathStep::Right, rest)) => (&[], false, rest, highlight_enabled),
                _ => (&[], false, &[], false),
            };
            render_expr(left, left_path, left_highlight, out, highlight_range);
            out.push_str(" - ");
            render_expr(right, right_path, right_highlight, out, highlight_range);
            out.push(')');
        }
        ExprKind::Mul(left, right) => {
            out.push('(');
            let (left_path, left_highlight, right_path, right_highlight): (
                &[PathStep],
                bool,
                &[PathStep],
                bool,
            ) = match highlight_path.split_first() {
                Some((PathStep::Left, rest)) => (rest, highlight_enabled, &[], false),
                Some((PathStep::Right, rest)) => (&[], false, rest, highlight_enabled),
                _ => (&[], false, &[], false),
            };
            render_expr(left, left_path, left_highlight, out, highlight_range);
            out.push_str(" * ");
            render_expr(right, right_path, right_highlight, out, highlight_range);
            out.push(')');
        }
    }
    let end = out.len();
    if highlight_enabled && highlight_path.is_empty() {
        *highlight_range = Some(start..end);
    }
}

fn path_in_subtree(path: &[PathStep], subtree: &[PathStep]) -> bool {
    path.len() >= subtree.len() && path[..subtree.len()] == *subtree
}

struct NodeDraw {
    label: String,
    depth: usize,
    x: i32,
    highlight: bool,
    children: Vec<usize>,
}

struct NodeLayout {
    rect: egui::Rect,
    label: String,
    highlight: bool,
    children: Vec<usize>,
}

fn build_nodes(
    expr: &Expr,
    depth: usize,
    path: &mut Vec<PathStep>,
    highlight_path: Option<&[PathStep]>,
    nodes: &mut Vec<NodeDraw>,
    next_leaf_x: &mut i32,
) -> usize {
    let highlight = highlight_path.map_or(false, |sub| path_in_subtree(path, sub));
    let (label, children, x) = match &expr.kind {
        ExprKind::Num(value) => {
            let x = *next_leaf_x;
            *next_leaf_x += 1;
            (value.to_string(), Vec::new(), x)
        }
        ExprKind::Neg(inner) => {
            path.push(PathStep::Unary);
            let child = build_nodes(inner, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            let x = nodes[child].x;
            ("-".to_string(), vec![child], x)
        }
        ExprKind::Add(left, right) => {
            path.push(PathStep::Left);
            let left_idx = build_nodes(left, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            path.push(PathStep::Right);
            let right_idx = build_nodes(right, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            let x = (nodes[left_idx].x + nodes[right_idx].x) / 2;
            ("+".to_string(), vec![left_idx, right_idx], x)
        }
        ExprKind::Sub(left, right) => {
            path.push(PathStep::Left);
            let left_idx = build_nodes(left, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            path.push(PathStep::Right);
            let right_idx = build_nodes(right, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            let x = (nodes[left_idx].x + nodes[right_idx].x) / 2;
            ("-".to_string(), vec![left_idx, right_idx], x)
        }
        ExprKind::Mul(left, right) => {
            path.push(PathStep::Left);
            let left_idx = build_nodes(left, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            path.push(PathStep::Right);
            let right_idx = build_nodes(right, depth + 1, path, highlight_path, nodes, next_leaf_x);
            path.pop();
            let x = (nodes[left_idx].x + nodes[right_idx].x) / 2;
            ("*".to_string(), vec![left_idx, right_idx], x)
        }
    };

    let index = nodes.len();
    nodes.push(NodeDraw {
        label,
        depth,
        x,
        highlight,
        children,
    });
    index
}

fn build_tree_layout(
    ui: &egui::Ui,
    expr: &Expr,
    highlight_path: Option<&[PathStep]>,
) -> (Vec<NodeLayout>, egui::Vec2, egui::FontId) {
    let mut nodes = Vec::new();
    let mut next_leaf_x = 0;
    let mut path = Vec::new();
    let _root = build_nodes(
        expr,
        0,
        &mut path,
        highlight_path,
        &mut nodes,
        &mut next_leaf_x,
    );

    let max_label_len = nodes
        .iter()
        .map(|node| node.label.chars().count())
        .max()
        .unwrap_or(1);
    let min_x = nodes.iter().map(|node| node.x).min().unwrap_or(0);
    let max_x = nodes.iter().map(|node| node.x).max().unwrap_or(0);
    let max_depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0);

    let font_id = TextStyle::Monospace.resolve(ui.style());
    let (char_width, row_height) = ui.fonts_mut(|fonts| {
        let width = fonts.glyph_width(&font_id, '0');
        let height = fonts.row_height(&font_id);
        (width.max(1.0), height.max(1.0))
    });
    let node_padding = egui::vec2((char_width * 0.6).max(4.0), (row_height * 0.2).max(2.0));
    let node_width = max_label_len as f32 * char_width + node_padding.x * 2.0;
    let node_height = row_height + node_padding.y * 2.0;
    let col_gap = (char_width * 2.0).max(8.0);
    let row_gap = (row_height * 0.8).max(8.0);
    let col_spacing = node_width + col_gap;
    let row_spacing = node_height + row_gap;

    let layout_width = node_width + (max_x - min_x) as f32 * col_spacing;
    let layout_height = node_height + max_depth as f32 * row_spacing;

    let mut layouts = Vec::with_capacity(nodes.len());
    for node in &nodes {
        let x_center = node_width / 2.0 + (node.x - min_x) as f32 * col_spacing;
        let y_center = node_height / 2.0 + node.depth as f32 * row_spacing;
        let rect =
            egui::Rect::from_center_size(egui::pos2(x_center, y_center), egui::vec2(node_width, node_height));
        layouts.push(NodeLayout {
            rect,
            label: node.label.clone(),
            highlight: node.highlight,
            children: node.children.clone(),
        });
    }

    (layouts, egui::vec2(layout_width, layout_height), font_id)
}

fn code_frame(ui: &mut egui::Ui, job: LayoutJob) {
    let bg = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.inactive.bg_stroke;
    egui::Frame::group(ui.style())
        .fill(bg)
        .stroke(stroke)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.label(job);
        });
}

fn highlight_formats(ui: &egui::Ui) -> (egui::TextFormat, egui::TextFormat) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let normal = egui::TextFormat::simple(font.clone(), ui.visuals().text_color());
    let highlight = egui::TextFormat::simple(font, GORBIE::themes::ral(2009));
    (normal, highlight)
}

fn append_highlighted_line(
    job: &mut LayoutJob,
    line: &str,
    ranges: &[Range<usize>],
    normal: &egui::TextFormat,
    highlight: &egui::TextFormat,
) {
    let mut cursor = 0;
    for range in ranges {
        let start = range.start.min(line.len());
        let end = range.end.min(line.len());
        if start > cursor {
            job.append(&line[cursor..start], 0.0, normal.clone());
        }
        if end > start {
            job.append(&line[start..end], 0.0, highlight.clone());
        }
        cursor = end;
    }
    if cursor < line.len() {
        job.append(&line[cursor..], 0.0, normal.clone());
    }
}

fn highlighted_job(ui: &egui::Ui, line: &str, ranges: &[Range<usize>]) -> LayoutJob {
    let (normal, highlight) = highlight_formats(ui);
    let mut job = LayoutJob::default();
    append_highlighted_line(&mut job, line, ranges, &normal, &highlight);
    job
}

fn tree_frame(ui: &mut egui::Ui, expr: &Expr, highlight_path: Option<&[PathStep]>) {
    let bg = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.inactive.bg_stroke;
    egui::Frame::group(ui.style())
        .fill(bg)
        .stroke(stroke)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(10.0)
        .show(ui, |ui| {
            draw_tree(ui, expr, highlight_path);
        });
}

fn draw_tree(ui: &mut egui::Ui, expr: &Expr, highlight_path: Option<&[PathStep]>) {
    let (mut layouts, desired, font_id) = build_tree_layout(ui, expr, highlight_path);
    let (rect, _response) = ui.allocate_at_least(desired, egui::Sense::hover());
    let mut origin = rect.min;
    if rect.width() > desired.x {
        origin.x += (rect.width() - desired.x) / 2.0;
    }
    if rect.height() > desired.y {
        origin.y += (rect.height() - desired.y) / 2.0;
    }

    for layout in &mut layouts {
        layout.rect = layout.rect.translate(origin.to_vec2());
    }

    let highlight_color = GORBIE::themes::ral(2009);
    let line_color = ui.visuals().widgets.inactive.bg_stroke.color;
    let line_width = ui.visuals().widgets.inactive.bg_stroke.width.max(1.0);
    let line_stroke = |highlight| {
        egui::Stroke::new(line_width, if highlight { highlight_color } else { line_color })
    };
    let text_color = ui.visuals().text_color();
    let painter = ui.painter();

    for layout in &layouts {
        for child_idx in &layout.children {
            let child = &layouts[*child_idx];
            let highlight = layout.highlight && child.highlight;
            let stroke = line_stroke(highlight);
            let start =
                layout.rect.center_bottom() + egui::vec2(0.0, stroke.width / 2.0);
            let end = child.rect.center_top() - egui::vec2(0.0, stroke.width / 2.0);
            let mid_y = (start.y + end.y) / 2.0;
            let points = vec![
                start,
                egui::pos2(start.x, mid_y),
                egui::pos2(end.x, mid_y),
                end,
            ];
            painter.add(egui::Shape::line(points, stroke));
        }
        let stroke = line_stroke(layout.highlight);
        painter.rect(
            layout.rect,
            egui::CornerRadius::same(4),
            ui.visuals().code_bg_color,
            stroke,
            egui::StrokeKind::Inside,
        );
        let color = if layout.highlight {
            highlight_color
        } else {
            text_color
        };
        let galley = ui
            .fonts_mut(|fonts| fonts.layout_no_wrap(layout.label.clone(), font_id.clone(), color));
        let text_pos = layout.rect.center() - galley.size() / 2.0;
        painter.galley(text_pos, galley, text_color);
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "# Hello, expressions\n\
                 An **expression** is a piece of code that produces a value.\n\n\
                 Examples:\n\
                 - `3`\n\
                 - `3 + 1`\n\
                 - `(10 - 4)`\n\
                 - `(3 * 2) + 2`\n\
                 - `-(4 + 1) * 3`\n\n\
                 Expressions do not change anything by themselves.\n\
                 They just *evaluate* to a value.\n\
                 **Evaluation** means turning an expression into a single value."
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## A tiny story\n\
                 Imagine two baskets of apples.\n\
                 Each basket holds 3 apples, and we have 2 baskets.\n\
                 So we can write `3 * 2` and get **6**.\n\n\
                 Now imagine there are 2 extra apples on the table:\n\
                 - First, multiply the baskets: `3 * 2`.\n\
                 - Then add the extras: `(3 * 2) + 2`.\n\n\
                 This is a **nested expression**: a smaller expression inside a bigger one.\n\
                 We start with the inside: `3 * 2` becomes `6`, then we finish `6 + 2`.\n\
                 We build it step by step, and the computer evaluates it the same way."
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Evaluation order\n\
                 When an expression has several operations, there are rules:\n\
                 - Parentheses first: `(3 + 2) * 4` evaluates the part inside `()` first.\n\
                 - Multiplication before addition or subtraction: `3 + 2 * 4` means `3 + (2 * 4)`.\n\
                 - Left-to-right when the precedence is the same: `8 - 3 - 2` means `(8 - 3) - 2`.\n\
                 - Unary minus sticks to the number or parentheses: `-(3 + 2)`.\n\n\
                 These rules are called **precedence** (what happens first) and\n\
                 **associativity** (how ties are grouped)."
            );
        });
    });

    nb.state("expression_state", ExpressionState::default(), |ui, state| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label(RichText::new("Step through an expression").heading());
            ui.add_space(4.0);
            ui.label("Use numbers, +, -, *, parentheses, and unary minus.");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("Expression:");
                let response = ui.add(widgets::TextField::singleline(&mut state.input));
                if response.changed() {
                    state.step = 0;
                }
            });

            let expr = match parse_expression(&state.input) {
                Ok(expr) => expr,
                Err(error) => {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("Parse error: {error}")).color(
                        ui.visuals().error_fg_color,
                    ));
                    return;
                }
            };

            let steps = match build_steps(expr) {
                Ok(steps) => steps,
                Err(error) => {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Evaluation error: {error}"))
                            .color(ui.visuals().error_fg_color),
                    );
                    return;
                }
            };

            let max_step = steps.len().saturating_sub(1);
            if state.step > max_step {
                state.step = max_step;
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(state.step > 0, widgets::Button::new("Prev"))
                    .clicked()
                {
                    state.step = state.step.saturating_sub(1);
                }
                if ui
                    .add_enabled(state.step < max_step, widgets::Button::new("Next"))
                    .clicked()
                {
                    state.step = (state.step + 1).min(max_step);
                }
                if ui.add(widgets::Button::new("Reset")).clicked() {
                    state.step = 0;
                }
                ui.add_space(6.0);
                ui.label(format!("Step {}/{}", state.step, max_step));
            });

            ui.add_space(8.0);
            let step = &steps[state.step];
            let (expression, expression_ranges) =
                render_expr_with_highlight(&step.expr, step.highlight.as_deref());
            code_frame(ui, highlighted_job(ui, &expression, &expression_ranges));

            ui.add_space(6.0);
            ui.label("Tree view:");
            ui.add_space(4.0);
            tree_frame(ui, &step.expr, step.highlight.as_deref());
            ui.add_space(6.0);
            if step.highlight.is_some() {
                ui.label("The highlighted part is what you can evaluate next.");
            } else {
                ui.label("Fully evaluated.");
            }
        });
    });

    nb.state("pair_state", PairState { a: 3, b: 2 }, |ui, state| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label(RichText::new("Try it yourself").heading());
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("a =");
                ui.add(
                    widgets::NumberField::new(&mut state.a)
                        .speed(1.0)
                        .min_decimals(0)
                        .max_decimals(0)
                        .constrain_value(&|_, proposed| proposed.clamp(-20, 20)),
                );
                ui.add_space(8.0);
                ui.label("b =");
                ui.add(
                    widgets::NumberField::new(&mut state.b)
                        .speed(1.0)
                        .min_decimals(0)
                        .max_decimals(0)
                        .constrain_value(&|_, proposed| proposed.clamp(-20, 20)),
                );
            });

            ui.add_space(8.0);
            let sum = state.a + state.b;
            let difference = state.a - state.b;
            let product = state.a * state.b;
            ui.label(format!("a + b = {sum}"));
            ui.label(format!("a - b = {difference}"));
            ui.label(format!("a * b = {product}"));
        });
    });

    nb.state("quiz_state", QuizState::default(), |ui, state| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label(RichText::new("Quick checks").heading());
            ui.add_space(6.0);

            ui.label("1) What does 6 + 2 evaluate to?");
            ui.add_space(4.0);
            ui.add(
                widgets::ChoiceToggle::new(&mut state.sum_result)
                    .choice(Some(7), "7")
                    .choice(Some(8), "8")
                    .choice(Some(9), "9")
                    .small(),
            );
            ui.add_space(4.0);
            match state.sum_result {
                Some(8) => ui.label("Correct!"),
                Some(_) => ui.label("Not quite. Try again."),
                None => ui.label("Pick an answer."),
            };

            ui.add_space(12.0);
            ui.label("2) What does (4 + 1) * 2 evaluate to?");
            ui.add_space(4.0);
            ui.add(
                widgets::ChoiceToggle::new(&mut state.product_result)
                    .choice(Some(8), "8")
                    .choice(Some(9), "9")
                    .choice(Some(10), "10")
                    .small(),
            );
            ui.add_space(4.0);
            match state.product_result {
                Some(10) => ui.label("Correct!"),
                Some(_) => ui.label("Try again. Evaluate inside the parentheses first."),
                None => ui.label("Pick an answer."),
            };
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## What just happened\n\
                 Expressions are little machines that turn inputs into values.\n\
                 You can use their results anywhere a number is needed.\n\n\
                 Next up: **Hello, state** shows how to *store* a value in a named box."
            );
        });
    });
}
