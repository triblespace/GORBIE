#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "../.." }
//! egui = "0.33"
//! ```

use GORBIE::cards::{with_padding, DEFAULT_CARD_PADDING};
use GORBIE::prelude::*;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "# Teaching notebooks plan\n\
                 A practical learning path for absolute beginners.\n\n\
                 This series is designed for learners with little or no formal math background.\n\
                 Every concept is grounded in simple language, concrete examples, and visible feedback."
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Principles\n\
                 - **Tiny steps**: one new idea per notebook.\n\
                 - **See the effect**: every concept has a visual or interactive demo.\n\
                 - **Practice > lecture**: short exercises after each demo.\n\
                 - **Build confidence**: celebrate correctness, then improve style.\n\
                 - **Vocabulary grows slowly**: define terms once and reuse them."
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Track A - Programming foundations (10-12 notebooks)\n\
                 1. Hello, expressions (values and math)\n\
                 2. Hello, state (variables and change)\n\
                 3. If/else and decisions\n\
                 4. Loops and counting\n\
                 5. Functions as reusable steps\n\
                 6. Lists and indexing\n\
                 7. Maps and lookup tables\n\
                 8. Debugging as a method\n\
                 9. Sorting and searching basics\n\
                 10. Complexity intuition (fast vs slow)\n\
                 11. Mini project: a tiny text game"
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Track B - Theoretical CS (10-12 notebooks)\n\
                 1. Sets, relations, and graphs\n\
                 2. Finite state machines (DFA)\n\
                 3. Regular expressions as machines\n\
                 4. Context-free grammars\n\
                 5. Parse trees by hand\n\
                 6. Turing machines (tape + rules)\n\
                 7. Halting problem intuition\n\
                 8. Reductions and NP overview\n\
                 9. Why some problems stay hard\n\
                 10. Mini project: build a tiny parser"
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Track C - Rust (12-15 notebooks)\n\
                 1. Ownership and moves\n\
                 2. Borrowing and references\n\
                 3. Structs, enums, and pattern matching\n\
                 4. Errors and `Result`\n\
                 5. Traits and generics (lightweight)\n\
                 6. Iterators and loops\n\
                 7. Strings and slices\n\
                 8. Modules and crates\n\
                 9. Concurrency basics\n\
                 10. Interior mutability\n\
                 11. Lifetimes intuition\n\
                 12. Mini project: a small CLI tool"
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Shared visual widgets\n\
                 - Stack and call-frame viewer\n\
                 - Memory map (owned vs borrowed)\n\
                 - Tape simulator (Turing machines)\n\
                 - Parse tree explorer\n\
                 - Stepper for algorithms\n\
                 - Tiny code runner with logs"
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Lesson template (every notebook)\n\
                 1. Short story or real-life analogy\n\
                 2. Minimal code demo\n\
                 3. Interactive widget\n\
                 4. Exercise (3-5 minutes)\n\
                 5. Recap in one paragraph"
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## Milestones\n\
                 - **Week 1**: basic variables, conditions, and loops\n\
                 - **Week 2**: functions + lists + small projects\n\
                 - **Week 3**: automata and parsing intuition\n\
                 - **Week 4**: Rust ownership and references\n\
                 - **Week 5**: build a mini project together"
            );
        });
    });

    nb.view(|ui| {
        note!(
            ui,
            "Start with four pilot notebooks:\n\
             - **Hello, expressions** (programming)\n\
             - **Hello, state** (programming)\n\
             - **DFA basics** (theory)\n\
             - **Ownership 101** (Rust)\n\n\
             We will test them, refine the language, and then expand."
        );
    });
}
