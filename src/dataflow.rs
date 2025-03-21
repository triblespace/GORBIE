use crate::CardState;

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

impl<A: Dependency + 'static> Dependencies for (CardState<A>,) {
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
    for (CardState<A>, CardState<B>)
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
    for (CardState<A>, CardState<B>, CardState<C>)
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
    > Dependencies for (CardState<A>, CardState<B>, CardState<C>, CardState<D>)
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
        CardState<H>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
        CardState<H>,
        CardState<I>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
        CardState<H>,
        CardState<I>,
        CardState<J>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
        CardState<H>,
        CardState<I>,
        CardState<J>,
        CardState<K>,
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
        CardState<A>,
        CardState<B>,
        CardState<C>,
        CardState<D>,
        CardState<E>,
        CardState<F>,
        CardState<G>,
        CardState<H>,
        CardState<I>,
        CardState<J>,
        CardState<K>,
        CardState<L>,
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
