// Lecture, côté callback de sortie, de l'entrée guitare poussée dans un ring par le callback
// d'entrée. En ASIO, l'entrée est servie juste avant la sortie dans le même cycle : on lit sans
// réserve. Ailleurs (WASAPI, CoreAudio, ALSA), les deux flux n'ont aucune relation de phase :
// on attend deux périodes d'avance avant de lire, et on rattrape la dérive des horloges.

/// Au-delà de `TRIM_PERIODS` périodes en attente (horloges d'entrée et de sortie qui dérivent),
/// on jette l'excédent pour revenir à `PREFILL_PERIODS` : un accroc ponctuel plutôt qu'une
/// latence qui grandit.
const PREFILL_PERIODS: usize = 2;
const TRIM_PERIODS: usize = 4;

pub struct InputFeed {
    ring: rtrb::Consumer<f32>,
    prefill: bool,
    /// Assez d'avance accumulée pour lire (toujours vrai sans pré-remplissage).
    primed: bool,
}

impl InputFeed {
    /// `prefill` : faux en ASIO (entrée et sortie dans le même cycle), vrai sinon.
    pub fn new(ring: rtrb::Consumer<f32>, prefill: bool) -> Self {
        InputFeed {
            ring,
            prefill,
            primed: !prefill,
        }
    }

    /// Jette tout ce qui attend (ouverture du flux) et repart de zéro.
    pub fn drain(&mut self) {
        let waiting = self.ring.slots();
        self.skip(waiting);
        self.primed = !self.prefill;
    }

    fn skip(&mut self, count: usize) {
        if let Ok(chunk) = self.ring.read_chunk(count) {
            chunk.commit_all();
        }
    }

    /// Remplit `out` (mono) ; le manque est du silence. Rend `true` sur une coupure (ring vide
    /// en cours de lecture), pas pendant l'attente du pré-remplissage.
    pub fn read(&mut self, out: &mut [f32]) -> bool {
        let period = out.len();
        if self.prefill {
            let waiting = self.ring.slots();
            if !self.primed {
                if waiting < PREFILL_PERIODS * period {
                    out.fill(0.0);
                    return false;
                }
                self.primed = true;
            }
            if waiting > TRIM_PERIODS * period {
                self.skip(waiting - PREFILL_PERIODS * period);
            }
        }
        let mut underrun = false;
        for sample in out.iter_mut() {
            *sample = self.ring.pop().unwrap_or_else(|_| {
                underrun = true;
                0.0
            });
        }
        if underrun {
            self.primed = !self.prefill;
        }
        underrun
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(prefill: bool) -> (rtrb::Producer<f32>, InputFeed) {
        let (producer, consumer) = rtrb::RingBuffer::new(1024);
        (producer, InputFeed::new(consumer, prefill))
    }

    fn push(producer: &mut rtrb::Producer<f32>, from: usize, count: usize) {
        for i in from..from + count {
            producer.push(i as f32).unwrap();
        }
    }

    #[test]
    fn without_prefill_reads_immediately() {
        let (mut tx, mut feed) = feed(false);
        push(&mut tx, 0, 4);
        let mut out = [9.0; 4];
        assert!(!feed.read(&mut out));
        assert_eq!(out, [0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn missing_samples_are_silence_and_reported() {
        let (mut tx, mut feed) = feed(false);
        push(&mut tx, 1, 2);
        let mut out = [9.0; 4];
        assert!(feed.read(&mut out));
        assert_eq!(out, [1.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn prefill_waits_for_two_periods() {
        let (mut tx, mut feed) = feed(true);
        push(&mut tx, 0, 4);
        let mut out = [9.0; 4];
        assert!(!feed.read(&mut out), "attente : pas une coupure");
        assert_eq!(out, [0.0; 4]);
        push(&mut tx, 4, 4);
        assert!(!feed.read(&mut out));
        assert_eq!(out, [0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn prefill_underrun_waits_again() {
        let (mut tx, mut feed) = feed(true);
        push(&mut tx, 0, 8);
        let mut out = [0.0; 4];
        feed.read(&mut out);
        feed.read(&mut out);
        assert!(feed.read(&mut out), "ring vide : coupure");
        push(&mut tx, 100, 4);
        assert!(!feed.read(&mut out));
        assert_eq!(out, [0.0; 4], "réamorçage : on attend deux périodes");
    }

    #[test]
    fn prefill_drops_excess_latency() {
        let (mut tx, mut feed) = feed(true);
        push(&mut tx, 0, 40);
        let mut out = [0.0; 4];
        assert!(!feed.read(&mut out));
        // 40 en attente pour une cible de 8 : on jette les 32 plus anciens.
        assert_eq!(out, [32.0, 33.0, 34.0, 35.0]);
    }

    #[test]
    fn drain_empties_the_ring() {
        let (mut tx, mut feed) = feed(false);
        push(&mut tx, 0, 10);
        feed.drain();
        push(&mut tx, 50, 2);
        let mut out = [0.0; 2];
        feed.read(&mut out);
        assert_eq!(out, [50.0, 51.0]);
    }
}
