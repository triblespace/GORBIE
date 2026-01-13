pub struct ComputedState<T> {
    value: T,
    in_flight: Option<std::thread::JoinHandle<T>>,
}

impl<T> ComputedState<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            in_flight: None,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn set(&mut self, value: T) {
        self.value = value;
        self.in_flight = None;
    }

    pub fn is_running(&self) -> bool {
        self.in_flight.is_some()
    }

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
        let handle = self
            .in_flight
            .take()
            .expect("in-flight handle missing");
        self.value = handle.join().unwrap();
        true
    }

    pub fn spawn(&mut self, action: impl FnOnce() -> T + Send + 'static) -> &mut T
    where
        T: Send + 'static,
    {
        self.spawn_if(|_| true, action)
    }

    pub fn spawn_if(
        &mut self,
        should_spawn: impl FnOnce(&T) -> bool,
        action: impl FnOnce() -> T + Send + 'static,
    ) -> &mut T
    where
        T: Send + 'static,
    {
        self.poll();
        if !self.is_running() && should_spawn(&self.value) {
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
