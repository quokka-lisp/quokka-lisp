//! Everything about a heap that is not one of its words: what an image
//! header carries beside them. Part of [`crate::heap`].

use super::*;

/// Everything about a heap that is not one of its words.
///
/// One `u64` per field, in the order [`crate::image`] writes them. A
/// deliberately dumb struct: adding a field here is adding a field to the
/// image format, which is exactly the coupling that should be impossible to
/// do by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scalars {
    pub sequence: u64,
    pub root: u64,
    pub obarray: u64,
    pub control: u64,
    pub env: u64,
    pub value: u64,
    pub stack: u64,
    pub unwind: u64,
    pub values: u64,
    pub function: u64,
    pub operands: u64,
    pub pc: u64,
    pub sp: u64,
    pub fp: u64,
    pub mp: u64,
    pub mode: u64,
    pub gensym_counter: u64,
    pub static_end: u64,
    /// Appended rather than placed beside the other registers, so that the
    /// words before it keep their positions and the byte offsets the image
    /// tests name stay true. Format version 2 (#35).
    pub request: u64,
    /// The running process (#34), appended for the same reason. Format
    /// version 10.
    pub process: u64,
}

impl Scalars {
    /// How many `u64`s a `Scalars` occupies in an image header.
    pub const WORDS: usize = 20;

    /// In the order they are written. Changing this order changes the format.
    pub fn to_words(self) -> [u64; Scalars::WORDS] {
        [
            self.sequence,
            self.root,
            self.obarray,
            self.control,
            self.env,
            self.value,
            self.stack,
            self.unwind,
            self.values,
            self.function,
            self.operands,
            self.pc,
            self.sp,
            self.fp,
            self.mp,
            self.mode,
            self.gensym_counter,
            self.static_end,
            self.request,
            self.process,
        ]
    }

    /// The inverse, and the only other place that knows the order.
    pub fn from_words(w: &[u64]) -> Scalars {
        Scalars {
            sequence: w[0],
            root: w[1],
            obarray: w[2],
            control: w[3],
            env: w[4],
            value: w[5],
            stack: w[6],
            unwind: w[7],
            values: w[8],
            function: w[9],
            operands: w[10],
            pc: w[11],
            sp: w[12],
            fp: w[13],
            mp: w[14],
            mode: w[15],
            gensym_counter: w[16],
            static_end: w[17],
            request: w[18],
            process: w[19],
        }
    }
}

impl Heap {
    /// Everything about the heap that is *not* in its words.
    ///
    /// The boundary that keeps the image format in one module: `heap` knows
    /// which fields exist, `image` knows what order they go on disk in, and
    /// neither knows the other's business. The same arrangement as the
    /// instruction encoding, where only the assembler and the VM's fetch know
    /// the byte layout.
    pub(crate) fn scalars(&self) -> Scalars {
        let s = &self.storage;
        Scalars {
            sequence: s.sequence,
            root: s.root,
            obarray: s.obarray,
            control: s.control,
            env: s.env,
            value: s.value,
            stack: s.stack,
            unwind: s.unwind,
            values: s.values,
            function: s.function,
            operands: s.operands,
            request: s.request,
            process: s.process,
            pc: s.pc as u64,
            sp: s.sp as u64,
            fp: s.fp as u64,
            // "No record" is `usize::MAX`, which is a different number on a
            // 32-bit machine. Written as it stood, a world saved in a browser
            // said record 4,294,967,295 to a 64-bit loader (#99); so the
            // image has one spelling for it, on every target.
            mp: if s.mp == usize::MAX {
                u64::MAX
            } else {
                s.mp as u64
            },
            mode: s.mode as u64,
            gensym_counter: s.gensym_counter,
            static_end: s.static_end as u64,
        }
    }

    /// Rebuild a heap from an image's words and scalars.
    ///
    /// Note what is *not* restored. `soft_limit` and `hard_limit` are the
    /// host's policy rather than the image's property — an image moved from a
    /// desktop to a phone should obey the phone's limits, not carry the
    /// desktop's — so they come from the caller. `collections` starts again at
    /// zero, because it counts this process's collections. `exhausted` is not
    /// saved at all: a heap that could not allocate is not a heap worth
    /// preserving, and restoring the flag would halt the evaluator for a
    /// reason that no longer applies.
    ///
    /// `dynamic_start` becomes `static_end`, deliberately dropping any torture
    /// padding the saving process had. The padding exists to move objects
    /// about between collections; it is not part of the world.
    pub(crate) fn from_image(words: Vec<u64>, s: Scalars, soft_limit: usize) -> Heap {
        let alive = words.len();
        Heap {
            storage: Storage {
                words,
                spare: Vec::new(),
                root: s.root,
                obarray: s.obarray,
                control: s.control,
                env: s.env,
                value: s.value,
                stack: s.stack,
                unwind: s.unwind,
                values: s.values,
                function: s.function,
                operands: s.operands,
                request: s.request,
                process: s.process,
                symbol_cache: Value::NIL.bits(),
                symbol_index: Value::NIL.bits(),
                pc: s.pc as usize,
                sp: s.sp as usize,
                fp: s.fp as usize,
                mp: if s.mp == u64::MAX {
                    usize::MAX
                } else {
                    s.mp as usize
                },
                mode: s.mode as u8,
                soft_limit,
                // An image is saved straight after a collection, so its
                // words are what was alive.
                trigger: soft_limit.max(alive.saturating_mul(2)),
                hard_limit: soft_limit.saturating_mul(HARD_LIMIT_MULTIPLE),
                exhausted: false,
                reserve_base: 0,
                collections: 0,
                live_after: 0,
                sequence: s.sequence,
                gensym_counter: s.gensym_counter,
                static_end: s.static_end as usize,
                dynamic_start: s.static_end as usize,
            },
        }
    }
}
