#![allow(non_snake_case)]

pub mod widgets;

use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use ctrlc;
use eframe::egui::{self, CollapsingHeader};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use parking_lot::RwLock;
use std::{
    collections::BTreeMap,
    sync::Arc,
};
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

pub enum Computed<T> {
    Undefined,
    Init(std::thread::JoinHandle<T>),
    Ready(T, usize),
    Stale(T, usize, std::thread::JoinHandle<T>),
}

impl<T> Computed<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Computed::Ready(inner, _) => Some(inner),
            Computed::Stale(inner, _, _) => Some(inner),
            _ => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            Computed::Ready(inner, _) => Some(inner),
            Computed::Stale(inner, _, _) => Some(inner),
            _ => None,
        }
    }
}

impl<T> std::default::Default for Computed<T> {
    fn default() -> Self {
        Computed::Undefined
    }
}

impl<T> std::fmt::Debug for Computed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Computed::Undefined => write!(f, "Undefined"),
            Computed::Init(_) => write!(f, "Loading"),
            Computed::Ready(_, _) => write!(f, "Ready"),
            Computed::Stale(_, _, _) => write!(f, "Refresh"),
        }
    }
}

pub trait Dependency<T> {
    fn generation(&self) -> Option<usize>;
    fn try_read(&self) -> Option<&T>;
}

impl<T> Dependency<T> for Computed<T> {
    fn generation(&self) -> Option<usize> {
        match self {
            Computed::Ready(_, generation) => Some(*generation),
            Computed::Stale(_, generation, _) => Some(*generation),
            _ => None,
        }
    }
    
    fn try_read(&self) -> Option<&T> {
        match self {
            Computed::Ready(inner, _) => Some(inner),
            Computed::Stale(inner, _, _) => Some(inner),
            _ => None,
        }
    }
    
}

pub trait Dependencies<'a> {
    type Guard;
    fn read(&'a self) -> Self::Guard;
}

impl<'a> Dependencies<'a> for () {
    type Guard = ();
    fn read(&'a self) -> Self::Guard {
        ()
    }
}

impl<'a, A: Send + 'static> Dependencies<'a> for (Arc<RwLock<A>>,) {
    type Guard = (parking_lot::RwLockReadGuard<'a, A>,);
    fn read(&'a self) -> Self::Guard {
        (self.0.read(),)
    }
}

impl<'a, A: Send + 'static, B: Send + 'static> Dependencies<'a>
    for (Arc<RwLock<A>>, Arc<RwLock<B>>)
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
    );
    fn read(&'a self) -> Self::Guard {
        (self.0.read(), self.1.read())
    }
}

impl<'a, A: Send + 'static, B: Send + 'static, C: Send + 'static> Dependencies<'a>
    for (Arc<RwLock<A>>, Arc<RwLock<B>>, Arc<RwLock<C>>)
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
    );
    fn read(&'a self) -> Self::Guard {
        (self.0.read(), self.1.read(), self.2.read())
    }
}

impl<'a, A: Send + 'static, B: Send + 'static, C: Send + 'static, D: Send + 'static>
    Dependencies<'a>
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
    );
    fn read(&'a self) -> Self::Guard {
        (self.0.read(), self.1.read(), self.2.read(), self.3.read())
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
    > Dependencies<'a>
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
    > Dependencies<'a>
    for (
        Arc<RwLock<A>>,
        Arc<RwLock<B>>,
        Arc<RwLock<C>>,
        Arc<RwLock<D>>,
        Arc<RwLock<E>>,
        Arc<RwLock<F>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
    > Dependencies<'a>
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
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
        M: Send + 'static,
    > Dependencies<'a>
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
        Arc<RwLock<M>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
        parking_lot::RwLockReadGuard<'a, M>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
            self.12.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
        M: Send + 'static,
        N: Send + 'static,
    > Dependencies<'a>
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
        Arc<RwLock<M>>,
        Arc<RwLock<N>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
        parking_lot::RwLockReadGuard<'a, M>,
        parking_lot::RwLockReadGuard<'a, N>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
            self.12.read(),
            self.13.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
        M: Send + 'static,
        N: Send + 'static,
        O: Send + 'static,
    > Dependencies<'a>
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
        Arc<RwLock<M>>,
        Arc<RwLock<N>>,
        Arc<RwLock<O>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
        parking_lot::RwLockReadGuard<'a, M>,
        parking_lot::RwLockReadGuard<'a, N>,
        parking_lot::RwLockReadGuard<'a, O>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
            self.12.read(),
            self.13.read(),
            self.14.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
        M: Send + 'static,
        N: Send + 'static,
        O: Send + 'static,
        P: Send + 'static,
    > Dependencies<'a>
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
        Arc<RwLock<M>>,
        Arc<RwLock<N>>,
        Arc<RwLock<O>>,
        Arc<RwLock<P>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
        parking_lot::RwLockReadGuard<'a, M>,
        parking_lot::RwLockReadGuard<'a, N>,
        parking_lot::RwLockReadGuard<'a, O>,
        parking_lot::RwLockReadGuard<'a, P>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
            self.12.read(),
            self.13.read(),
            self.14.read(),
            self.15.read(),
        )
    }
}

impl<
        'a,
        A: Send + 'static,
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        G: Send + 'static,
        H: Send + 'static,
        I: Send + 'static,
        J: Send + 'static,
        K: Send + 'static,
        L: Send + 'static,
        M: Send + 'static,
        N: Send + 'static,
        O: Send + 'static,
        P: Send + 'static,
        Q: Send + 'static,
    > Dependencies<'a>
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
        Arc<RwLock<M>>,
        Arc<RwLock<N>>,
        Arc<RwLock<O>>,
        Arc<RwLock<P>>,
        Arc<RwLock<Q>>,
    )
{
    type Guard = (
        parking_lot::RwLockReadGuard<'a, A>,
        parking_lot::RwLockReadGuard<'a, B>,
        parking_lot::RwLockReadGuard<'a, C>,
        parking_lot::RwLockReadGuard<'a, D>,
        parking_lot::RwLockReadGuard<'a, E>,
        parking_lot::RwLockReadGuard<'a, F>,
        parking_lot::RwLockReadGuard<'a, G>,
        parking_lot::RwLockReadGuard<'a, H>,
        parking_lot::RwLockReadGuard<'a, I>,
        parking_lot::RwLockReadGuard<'a, J>,
        parking_lot::RwLockReadGuard<'a, K>,
        parking_lot::RwLockReadGuard<'a, L>,
        parking_lot::RwLockReadGuard<'a, M>,
        parking_lot::RwLockReadGuard<'a, N>,
        parking_lot::RwLockReadGuard<'a, O>,
        parking_lot::RwLockReadGuard<'a, P>,
        parking_lot::RwLockReadGuard<'a, Q>,
    );
    fn read(&'a self) -> Self::Guard {
        (
            self.0.read(),
            self.1.read(),
            self.2.read(),
            self.3.read(),
            self.4.read(),
            self.5.read(),
            self.6.read(),
            self.7.read(),
            self.8.read(),
            self.9.read(),
            self.10.read(),
            self.11.read(),
            self.12.read(),
            self.13.read(),
            self.14.read(),
            self.15.read(),
            self.16.read(),
        )
    }
}

pub struct ReactiveCard<T: Send, D: for<'a> Dependencies<'a> + Send> {
    value: Arc<RwLock<Computed<T>>>,
    dependencies: D,
    function: Arc<dyn Fn(<D as Dependencies<'_>>::Guard) -> T + Send + Sync>,
    code: Option<String>,
}

pub fn reactive_card<
    T: Send + PartialEq + std::fmt::Debug + std::default::Default + 'static,
    D: for<'a> Dependencies<'a> + Send + Clone + 'static,
>(
    nb: &mut Notebook,
    dependencies: D,
    function: impl Fn(<D as Dependencies<'_>>::Guard) -> T + Send + Sync + 'static,
    code: Option<&str>,
) -> Arc<RwLock<Computed<T>>> {
    let current = Arc::new(RwLock::new(Computed::Undefined));
    nb.push_card(Box::new(ReactiveCard {
        value: current.clone(),
        dependencies,
        function: Arc::new(function),
        code: code.map(|s| s.to_owned()),
    }));

    current
}

impl<
        T: Send + std::fmt::Debug + PartialEq + 'static,
        D: for<'a> Dependencies<'a> + Send + Clone + 'static,
    > Card for ReactiveCard<T, D>
{
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let mut current = self.value.write();

        *current = match std::mem::replace(&mut *current, Computed::Undefined) {
            Computed::Undefined => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                Computed::Init(std::thread::spawn(move || {
                    let dependencies = dependencies.read();
                    (function)(dependencies)
                }))
            }

            Computed::Init(handle) if handle.is_finished() => {
                ctx.ui.ctx().request_repaint();
                Computed::Ready(handle.join().unwrap(), 0)
            }

            Computed::Init(handle) => {
                ctx.ui.add(egui::widgets::Spinner::new());
                Computed::Init(handle)
            }

            Computed::Ready(current, generation) => {
                ctx.ui.label(format!("Generation: {}", generation));

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
                            egui_extras::syntax_highlighting::code_view_ui(
                                ui, &theme, code, language,
                            );
                        });
                }

                Computed::Ready(current, generation)
            }
            Computed::Stale(previous, generation, join_handle) if join_handle.is_finished() => {
                let result = join_handle.join().unwrap();
                if result != previous {
                    ctx.ui.ctx().request_repaint();
                    Computed::Ready(result, generation + 1)
                } else {
                    Computed::Ready(result, generation)
                }
            }

            Computed::Stale(current, generation, join_handle) => {
                ctx.ui.add(egui::widgets::Spinner::new());

                ctx.ui.label(format!("Generation: {}", generation));

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
                            egui_extras::syntax_highlighting::code_view_ui(
                                ui, &theme, code, language,
                            );
                        });
                }

                Computed::Stale(current, generation, join_handle)
            }
        }
    }
}

#[macro_export]
macro_rules! react {
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
