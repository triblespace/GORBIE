pub trait Dependency {
    type Value;

    fn generation(&self) -> Option<usize>;
    fn ready(&self) -> Option<Self::Value>;
}

#[derive(Default)]
pub enum ComputedState<T> {
    #[default]
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
