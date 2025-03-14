#![allow(non_snake_case)]

pub mod widgets;

use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use ctrlc;
use eframe::egui::{self, CollapsingHeader};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use parking_lot::RwLock;
use std::{collections::BTreeMap, ops::Deref, sync::Arc};
use tribles::prelude::*;

pub struct CardCtx<'a> {
    pub ui: &'a mut egui::Ui,
    id: Id,
}

impl CardCtx<'_> {
    pub fn id(&self) -> Id {
        self.id
    }
}

pub trait Card {
    fn update(&mut self, ctx: &mut CardCtx) -> ();
}

pub struct MarkdownCard {
    markdown: String,
    cache: CommonMarkCache,
}

impl Card for MarkdownCard {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        CommonMarkViewer::new().show(ctx.ui, &mut self.cache, &self.markdown);
    }
}

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut CardCtx) -> ()>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        (self.function)(ctx);

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt(format!("{:x}/code", ctx.id))
                .show(ctx.ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut CardCtx) -> () + 'static,
    code: Option<&str>,
) {
    nb.push_card(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
}

#[macro_export]
macro_rules! view {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::stateless_card($nb, $code, Some(stringify!($code)))
        }
    };
}

pub struct StatefulCard<T> {
    current: Arc<RwLock<T>>,
    function: Box<dyn FnMut(&mut CardCtx, &mut T)>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let mut current = self.current.write();
        (self.function)(ctx, &mut current);

        CollapsingHeader::new("Current")
            .id_salt("__current")
            .show(ctx.ui, |ui| {
                ui.monospace(format!("{:?}", current));
            });

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt("__code")
                .show(ctx.ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }
}

type CardState<T> = Arc<RwLock<T>>;

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut CardCtx, &mut T) + 'static,
    code: Option<&str>,
) -> CardState<T> {
    let current = Arc::new(RwLock::new(init));
    nb.push_card(Box::new(StatefulCard {
        current: current.clone(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));

    current
}

#[macro_export]
macro_rules! state {
    ($nb:expr, $code:expr) => {
        $crate::stateful_card($nb, Default::default(), $code, Some(stringify!($code)))
    };
    ($nb:expr, $init:expr, $code:expr) => {
        $crate::stateful_card($nb, $init, $code, Some(stringify!($code)))
    };
}

pub trait Dependency {
    type Value;

    fn generation(&self) -> Option<usize>;
    fn ready(&self) -> Option<Self::Value>;
}

pub enum ComputedState<T> {
    Undefined,
    Init(std::thread::JoinHandle<T>),
    Ready(T, usize),
    Stale(T, usize, std::thread::JoinHandle<T>),
}

impl<T> ComputedState<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            ComputedState::Ready(inner, _) => Some(inner),
            ComputedState::Stale(inner, _, _) => Some(inner),
            _ => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            ComputedState::Ready(inner, _) => Some(inner),
            ComputedState::Stale(inner, _, _) => Some(inner),
            _ => None,
        }
    }
}

impl<T: Clone> Dependency for ComputedState<T> {
    type Value = T;

    fn generation(&self) -> Option<usize> {
        match self {
            ComputedState::Ready(_, generation) => Some(*generation),
            ComputedState::Stale(_, generation, _) => Some(*generation),
            _ => None,
        }
    }

    fn ready(&self) -> Option<T> {
        self.ready().cloned()
    }
}

impl<T> std::default::Default for ComputedState<T> {
    fn default() -> Self {
        ComputedState::Undefined
    }
}

impl<T> std::fmt::Debug for ComputedState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputedState::Undefined => write!(f, "Undefined"),
            ComputedState::Init(_) => write!(f, "Init"),
            ComputedState::Ready(_, _) => write!(f, "Ready"),
            ComputedState::Stale(_, _, _) => write!(f, "Refresh"),
        }
    }
}

pub struct NotifiedState<T> {
    value: T,
    generation: usize,
}

impl<T: Clone> Dependency for NotifiedState<T> {
    type Value = T;

    fn generation(&self) -> Option<usize> {
        Some(self.generation)
    }

    fn ready(&self) -> Option<T> {
        Some(self.value.clone())
    }
}

impl<T> NotifiedState<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            generation: 0,
        }
    }

    pub fn notify(&mut self) {
        self.generation += 1;
    }
}

impl<T> std::default::Default for NotifiedState<T>
where
    T: std::default::Default,
{
    fn default() -> Self {
        Self {
            value: T::default(),
            generation: 0,
        }
    }
}

impl<T> std::ops::Deref for NotifiedState<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for NotifiedState<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> std::fmt::Debug for NotifiedState<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifiedState")
            .field("value", &self.value)
            .finish()
    }
}

impl<T> From<T> for NotifiedState<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

pub trait Dependencies {
    type Values;
    type Generations: PartialEq;
    fn read(&self) -> Option<Self::Values>;
    fn try_read(&self) -> Option<Self::Values>;
    fn try_generations(&self) -> Option<Self::Generations>;
}

impl Dependencies for () {
    type Values = ();
    fn read(&self) -> Option<Self::Values> {
        Some(())
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some(())
    }

    type Generations = ();
    fn try_generations(&self) -> Option<Self::Generations> {
        Some(())
    }
}

impl<A: Dependency + 'static> Dependencies for (Arc<RwLock<A>>,) {
    type Values = (A::Value,);
    fn read(&self) -> Option<Self::Values> {
        Some((self.0.read().ready()?,))
    }
    fn try_read<'a>(&'a self) -> Option<Self::Values> {
        Some((self.0.try_read()?.ready()?,))
    }

    type Generations = (usize,);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((self.0.try_read()?.generation()?,))
    }
}

impl<A: Dependency + 'static, B: Dependency + 'static> Dependencies
    for (Arc<RwLock<A>>, Arc<RwLock<B>>)
{
    type Values = (A::Value, B::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((self.0.read().ready()?, self.1.read().ready()?))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((self.0.try_read()?.ready()?, self.1.try_read()?.ready()?))
    }

    type Generations = (usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
        ))
    }
}

impl<A: Dependency + 'static, B: Dependency + 'static, C: Dependency + 'static> Dependencies
    for (Arc<RwLock<A>>, Arc<RwLock<B>>, Arc<RwLock<C>>)
{
    type Values = (A::Value, B::Value, C::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
        ))
    }

    type Generations = (usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
    )
{
    type Values = (A::Value, B::Value, C::Value, D::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
        ))
    }

    type Generations = (usize, usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
    )
{
    type Values = (A::Value, B::Value, C::Value, D::Value, E::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
        ))
    }

    type Generations = (usize, usize, usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
    )
{
    type Values = (A::Value, B::Value, C::Value, D::Value, E::Value, F::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
        ))
    }

    type Generations = (usize, usize, usize, usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
    )
{
    type Values = (A::Value, B::Value, C::Value, D::Value, E::Value, F::Value, G::Value);
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
        ))
    }


    type Generations = (usize, usize, usize, usize, usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
        H: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
        Arc<RwLock<H>>,
    )
{
    type Values = (
        A::Value,
        B::Value,
        C::Value,
        D::Value,
        E::Value,
        F::Value,
        G::Value,
        H::Value,
    );
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
            self.7.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
            self.7.try_read()?.ready()?,
        ))
    }

    type Generations = (usize, usize, usize, usize, usize, usize, usize, usize);
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
            self.7.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
        H: Dependency + 'static,
        I: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
        Arc<RwLock<H>>,
        Arc<RwLock<I>>,
    )
{
    type Values = (
        A::Value,
        B::Value,
        C::Value,
        D::Value,
        E::Value,
        F::Value,
        G::Value,
        H::Value,
        I::Value,
    );
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
            self.7.read().ready()?,
            self.8.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
            self.7.try_read()?.ready()?,
            self.8.try_read()?.ready()?,
        ))
    }

    type Generations = (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    );
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
            self.7.try_read()?.generation()?,
            self.8.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
        H: Dependency + 'static,
        I: Dependency + 'static,
        J: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
        Arc<RwLock<H>>,
        Arc<RwLock<I>>,
        Arc<RwLock<J>>,
    )
{
    type Values = (
        A::Value,
        B::Value,
        C::Value,
        D::Value,
        E::Value,
        F::Value,
        G::Value,
        H::Value,
        I::Value,
        J::Value,
    );
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
            self.7.read().ready()?,
            self.8.read().ready()?,
            self.9.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
            self.7.try_read()?.ready()?,
            self.8.try_read()?.ready()?,
            self.9.try_read()?.ready()?,
        ))
    }

    type Generations = (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    );
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
            self.7.try_read()?.generation()?,
            self.8.try_read()?.generation()?,
            self.9.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
        H: Dependency + 'static,
        I: Dependency + 'static,
        J: Dependency + 'static,
        K: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
        Arc<RwLock<H>>,
        Arc<RwLock<I>>,
        Arc<RwLock<J>>,
        Arc<RwLock<K>>,
    )
{
    type Values = (
        A::Value,
        B::Value,
        C::Value,
        D::Value,
        E::Value,
        F::Value,
        G::Value,
        H::Value,
        I::Value,
        J::Value,
        K::Value,
    );
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
            self.7.read().ready()?,
            self.8.read().ready()?,
            self.9.read().ready()?,
            self.10.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
            self.7.try_read()?.ready()?,
            self.8.try_read()?.ready()?,
            self.9.try_read()?.ready()?,
            self.10.try_read()?.ready()?,
        ))
    }

    type Generations = (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    );
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
            self.7.try_read()?.generation()?,
            self.8.try_read()?.generation()?,
            self.9.try_read()?.generation()?,
            self.10.try_read()?.generation()?,
        ))
    }
}

impl<
        A: Dependency + 'static,
        B: Dependency + 'static,
        C: Dependency + 'static,
        D: Dependency + 'static,
        E: Dependency + 'static,
        F: Dependency + 'static,
        G: Dependency + 'static,
        H: Dependency + 'static,
        I: Dependency + 'static,
        J: Dependency + 'static,
        K: Dependency + 'static,
        L: Dependency + 'static,
    > Dependencies
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
        Arc<RwLock<G>>,
        Arc<RwLock<H>>,
        Arc<RwLock<I>>,
        Arc<RwLock<J>>,
        Arc<RwLock<K>>,
        Arc<RwLock<L>>,
    )
{
    type Values = (
        A::Value,
        B::Value,
        C::Value,
        D::Value,
        E::Value,
        F::Value,
        G::Value,
        H::Value,
        I::Value,
        J::Value,
        K::Value,
        L::Value,
    );
    fn read(&self) -> Option<Self::Values> {
        Some((
            self.0.read().ready()?,
            self.1.read().ready()?,
            self.2.read().ready()?,
            self.3.read().ready()?,
            self.4.read().ready()?,
            self.5.read().ready()?,
            self.6.read().ready()?,
            self.7.read().ready()?,
            self.8.read().ready()?,
            self.9.read().ready()?,
            self.10.read().ready()?,
            self.11.read().ready()?,
        ))
    }
    fn try_read(&self) -> Option<Self::Values> {
        Some((
            self.0.try_read()?.ready()?,
            self.1.try_read()?.ready()?,
            self.2.try_read()?.ready()?,
            self.3.try_read()?.ready()?,
            self.4.try_read()?.ready()?,
            self.5.try_read()?.ready()?,
            self.6.try_read()?.ready()?,
            self.7.try_read()?.ready()?,
            self.8.try_read()?.ready()?,
            self.9.try_read()?.ready()?,
            self.10.try_read()?.ready()?,
            self.11.try_read()?.ready()?,
        ))
    }

    type Generations = (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    );
    fn try_generations(&self) -> Option<Self::Generations> {
        Some((
            self.0.try_read()?.generation()?,
            self.1.try_read()?.generation()?,
            self.2.try_read()?.generation()?,
            self.3.try_read()?.generation()?,
            self.4.try_read()?.generation()?,
            self.5.try_read()?.generation()?,
            self.6.try_read()?.generation()?,
            self.7.try_read()?.generation()?,
            self.8.try_read()?.generation()?,
            self.9.try_read()?.generation()?,
            self.10.try_read()?.generation()?,
            self.11.try_read()?.generation()?,
        ))
    }
}

pub struct ReactiveCard<T: Send, D: for<'a> Dependencies + Send> {
    value: Arc<RwLock<ComputedState<T>>>,
    generations: Option<<D as Dependencies>::Generations>,
    dependencies: D,
    function: Arc<dyn Fn(<D as Dependencies>::Values) -> T + Send + Sync>,
    code: Option<String>,
}

pub fn reactive_card<
    T: Send + PartialEq + std::fmt::Debug + std::default::Default + 'static,
    D: for<'a> Dependencies + Send + Clone + 'static,
>(
    nb: &mut Notebook,
    dependencies: D,
    function: impl Fn(<D as Dependencies>::Values) -> T + Send + Sync + 'static,
    code: Option<&str>,
) -> Arc<RwLock<ComputedState<T>>> {
    let current = Arc::new(RwLock::new(ComputedState::Undefined));
    nb.push_card(Box::new(ReactiveCard {
        value: current.clone(),
        generations: None,
        dependencies,
        function: Arc::new(function),
        code: code.map(|s| s.to_owned()),
    }));

    current
}

impl<T: Send + std::fmt::Debug + PartialEq + 'static, D: Dependencies + Send + Clone + 'static> Card
    for ReactiveCard<T, D>
{
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let mut current = self.value.write();
        *current = match std::mem::replace(&mut *current, ComputedState::Undefined) {
            ComputedState::Undefined => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() {
                    self.generations = generations;
                    ComputedState::Init(std::thread::spawn(move || {
                        let dependencies = dependencies.read().expect("failed to read dependencies, although generations were available");
                        (function)(dependencies)
                    }))
                } else {
                    ComputedState::Undefined
                }
            }

            ComputedState::Init(handle) if handle.is_finished() => {
                ctx.ui.ctx().request_repaint();
                ComputedState::Ready(handle.join().unwrap(), 0)
            }

            ComputedState::Init(handle) => {
                ComputedState::Init(handle)
            }

            ComputedState::Ready(current, generation) => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() && generations != self.generations {
                    self.generations = generations;
                    ComputedState::Stale(current, generation, std::thread::spawn(move || {
                        let dependencies = dependencies.read().expect("failed to read dependencies, although generations were available");
                        (function)(dependencies)
                    }))
                } else {
                    ComputedState::Ready(current, generation)
                }
            }
            ComputedState::Stale(previous, generation, join_handle)
                if join_handle.is_finished() =>
            {
                let result = join_handle.join().unwrap();
                if result != previous {
                    ctx.ui.ctx().request_repaint();
                    ComputedState::Ready(result, generation + 1)
                } else {
                    ComputedState::Ready(result, generation)
                }
            }

            ComputedState::Stale(current, generation, join_handle) => {
                ComputedState::Stale(current, generation, join_handle)
            }
        };

        CollapsingHeader::new("Current")
        .id_salt("__current")
        .show(ctx.ui, |ui| {
            match current.deref() {
                ComputedState::Ready(value, _) => {
                    ui.monospace(format!("{:?}", value));
                }
                _ => {
                    ui.add(egui::widgets::Spinner::new());
                }
            }
        });

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt("__code")
                .show(ctx.ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(
                        ui, &theme, code, language,
                    );
                });
        }
    }
}

#[macro_export]
macro_rules! derive {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::reactive_card($nb, ($($Dep),*,), $code, Some(stringify!($code)))
        }
    };
}

pub struct Notebook {
    pub cards: Vec<(Id, Box<dyn Card>)>,
}

pub fn md(nb: &mut Notebook, markdown: &str) {
    nb.push_card(Box::new(MarkdownCard {
        markdown: markdown.to_owned(),
        cache: CommonMarkCache::default(),
    }));
}

impl Notebook {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn push_card(&mut self, card: Box<dyn Card>) {
        self.cards.push((*fucid(), card));
    }

    pub fn run(self, name: &str) -> eframe::Result {
        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;

        eframe::run_native(
            name,
            native_options,
            Box::new(|cc| {
                let ctx = cc.egui_ctx.clone();
                ctrlc::set_handler(move || ctx.send_viewport_cmd(egui::ViewportCommand::Close))
                    .expect("failed to set exit signal handler");

                let mut fonts = FontDefinitions::default();
                fonts.font_data.insert(
                    "lora".to_owned(),
                    std::sync::Arc::new(FontData::from_static(include_bytes!(
                        "../assets/fonts/Lora/Lora-VariableFont_wght.ttf"
                    ))),
                );
                fonts.font_data.insert("atkinson".to_owned(),
                    std::sync::Arc::new(
                        FontData::from_static(include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/AtkinsonHyperlegibleNext-VariableFont_wght.ttf"))
                    )
                    );
                fonts.font_data.insert(
                    "roboto_mono".to_owned(),
                    std::sync::Arc::new(FontData::from_static(include_bytes!(
                        "../assets/fonts/Roboto_Mono/RobotoMono-VariableFont_wght.ttf"
                    ))),
                );
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "atkinson".to_owned());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "roboto_mono".to_owned());

                fonts
                    .families
                    .insert(FontFamily::Name("lora".into()), vec!["lora".into()]);
                fonts
                    .families
                    .insert(FontFamily::Name("atkinson".into()), vec!["atkinson".into()]);
                fonts.families.insert(
                    FontFamily::Name("roboto_mono".into()),
                    vec!["roboto_mono".into()],
                );

                cc.egui_ctx.set_fonts(fonts);

                let text_styles: BTreeMap<_, _> = [
                    (
                        TextStyle::Heading,
                        FontId::new(32.0, FontFamily::Name("lora".into())),
                    ),
                    (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
                    (
                        TextStyle::Monospace,
                        FontId::new(16.0, FontFamily::Monospace),
                    ),
                    (
                        TextStyle::Button,
                        FontId::new(16.0, FontFamily::Proportional),
                    ),
                    (
                        TextStyle::Small,
                        FontId::new(12.0, FontFamily::Proportional),
                    ),
                ]
                .into();

                cc.egui_ctx.all_styles_mut(move |style| {
                    style.text_styles = text_styles.clone();

                    // Base color: #130496
                    // Fade to white:
                    // #130496 #5032a8 #7858ba #9b80cc #bda9dd #ded3ee #ffffff
                    // Fade to black:
                    // #130496 #1a087b #1c0b62 #1a0c4a #170b32 #12051d #000000
                    style.visuals.window_fill = egui::Color32::from_hex("#ffffff").unwrap();
                    style.visuals.panel_fill = egui::Color32::from_hex("#ffffff").unwrap();
                    style.visuals.faint_bg_color = egui::Color32::from_hex("#ded3ee").unwrap();
                    style.visuals.extreme_bg_color = egui::Color32::from_hex("#9b80cc").unwrap();
                    style.visuals.code_bg_color = egui::Color32::from_hex("#ded3ee").unwrap();
                    style.visuals.selection.bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
                    style.visuals.hyperlink_color = egui::Color32::from_hex("#130496").unwrap();
                    style.visuals.warn_fg_color = egui::Color32::from_hex("#1a087b").unwrap();
                    style.visuals.error_fg_color = egui::Color32::from_hex("#1c0b62").unwrap();
                    style.visuals.widgets.active.fg_stroke.color =
                        egui::Color32::from_hex("#170b32").unwrap();
                    style.visuals.override_text_color =
                        Some(egui::Color32::from_hex("#12051d").unwrap());
                });

                Ok(Box::new(self))
            }),
        )
    }
}

impl eframe::App for Notebook {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(740.0);
                        for (id, card) in &mut self.cards {
                            ui.push_id(&id, |ui| {
                                let mut ctx = CardCtx { ui, id: *id };
                                card.update(&mut ctx);
                                ui.separator();
                            });
                        }
                    });
                });
        });
    }
}

#[macro_export]
macro_rules! notebook {
    ($setup:ident) => {
        let mut notebook = Notebook::new();
        $setup(&mut notebook);

        let this_file = file!();
        let filename = std::path::Path::new(this_file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap();

        notebook.run(filename).unwrap();
    };
}
