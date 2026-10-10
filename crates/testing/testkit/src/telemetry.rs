use mivia_ports::telemetry::{ProductEvent, TelemetrySink};
use parking_lot::Mutex;

#[derive(Default)]
pub struct RecordingTelemetry {
    events: Mutex<Vec<ProductEvent>>,
}

impl RecordingTelemetry {
    pub fn events(&self) -> Vec<ProductEvent> {
        self.events.lock().clone()
    }
}

impl TelemetrySink for RecordingTelemetry {
    fn record(&self, event: ProductEvent) {
        self.events.lock().push(event);
    }
}
