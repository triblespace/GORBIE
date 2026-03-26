/// Holds a value that can be recomputed asynchronously in a background thread.
///
/// Use [`spawn`](Self::spawn) to kick off a computation and [`poll`](Self::poll)
/// to check for completion. The current value remains accessible while a new one
/// is being computed.
pub struct ComputedState<T> {
    value: T,
    in_flight: Option<std::thread::JoinHandle<T>>,
}

impl<T> ComputedState<T> {
    /// Creates a new `ComputedState` with the given initial value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            in_flight: None,
        }
    }

    /// Returns a shared reference to the current value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns a mutable reference to the current value.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Replaces the current value and cancels any in-flight computation.
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.in_flight = None;
    }

    /// Returns `true` if a background computation is in progress.
    pub fn is_running(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Checks whether the in-flight computation has finished.
    /// If so, updates the stored value and returns `true`.
    pub fn poll(&mut self) -> bool
    where
        T: Send + 'static,
    {
        let Some(handle) = self.in_flight.as_ref() else {
            return false;
        };
        if !handle.is_finished() {
            return false;
        }
        let handle = self.in_flight.take().expect("in-flight handle missing");
        self.value = handle.join().unwrap();
        true
    }

    /// Spawns `action` on a background thread if no computation is already running.
    /// Returns a mutable reference to the current (potentially stale) value.
    pub fn spawn(&mut self, action: impl FnOnce() -> T + Send + 'static) -> &mut T
    where
        T: Send + 'static,
    {
        self.poll();
        if !self.is_running() {
            self.in_flight = Some(std::thread::spawn(action));
        }
        &mut self.value
    }
}

impl<T: Default> Default for ComputedState<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ComputedState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputedState")
            .field("value", &self.value)
            .field("running", &self.is_running())
            .finish()
    }
}
