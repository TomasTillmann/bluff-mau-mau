//! CPython's integer-seeded MT19937, including getrandbits-based shuffles.
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

#[derive(Clone, Debug, PartialEq)]
pub struct RngState {
    version: u8,
    words: [u32; 624],
    index: usize,
    gauss: Option<f64>,
    // Keep unusual accepted external state words verbatim until RNG use canonicalizes them.
    raw_words: Option<Vec<Number>>,
}

impl Default for RngState {
    fn default() -> Self {
        PythonRandom::seed(0).state()
    }
}

impl Serialize for RngState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut internal = self
            .raw_words
            .clone()
            .unwrap_or_else(|| self.words.iter().map(|&w| Number::from(w)).collect());
        internal.push(Number::from(self.index));
        let mut tuple = serializer.serialize_tuple(3)?;
        tuple.serialize_element(&self.version)?;
        tuple.serialize_element(&internal)?;
        tuple.serialize_element(&self.gauss)?;
        tuple.end()
    }
}

fn modulo_word(number: &Number) -> Result<u32, String> {
    let text = number.to_string();
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("RNG words must be integers".into());
    }
    let value = digits.bytes().fold(0u32, |v, b| {
        v.wrapping_mul(10).wrapping_add((b - b'0') as u32)
    });
    Ok(if text.starts_with('-') {
        value.wrapping_neg()
    } else {
        value
    })
}

impl<'de> Deserialize<'de> for RngState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let (version, mut internal, raw_gauss): (u8, Vec<Number>, Option<Number>) =
            Deserialize::deserialize(deserializer)?;
        let gauss = match raw_gauss {
            Some(number) if number.to_string().contains(['.', 'e', 'E']) => Some(
                number
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| D::Error::custom("Invalid Gaussian cache"))?,
            ),
            Some(_) => return Err(D::Error::custom("Gaussian cache must be a float")),
            None => None,
        };
        if !matches!(version, 2 | 3) || internal.len() != 625 {
            return Err(D::Error::custom("Invalid random-generator state"));
        }
        let index = internal
            .pop()
            .and_then(|n| n.as_u64())
            .filter(|&n| n <= 624)
            .ok_or_else(|| D::Error::custom("Invalid random-generator index"))?
            as usize;
        let mut words = [0u32; 624];
        for (word, number) in words.iter_mut().zip(&internal) {
            *word = if version == 2 {
                modulo_word(number).map_err(D::Error::custom)?
            } else {
                number
                    .as_u64()
                    .ok_or_else(|| D::Error::custom("Invalid random-generator word"))?
                    as u32
            };
        }
        let noncanonical = internal
            .iter()
            .zip(words)
            .any(|(raw, normalized)| raw.as_u64() != Some(normalized as u64));
        Ok(Self {
            version,
            words,
            index,
            gauss,
            raw_words: noncanonical.then_some(internal),
        })
    }
}

#[derive(Clone, Debug)]
pub struct PythonRandom {
    words: [u32; 624],
    index: usize,
    gauss: Option<f64>,
}

impl PythonRandom {
    pub fn seed(seed: u64) -> Self {
        let keys = [seed as u32, (seed >> 32) as u32];
        Self::seed_words(if keys[1] == 0 { &keys[..1] } else { &keys })
    }

    pub fn seed_signed(seed: i64) -> Self {
        Self::seed(seed.unsigned_abs())
    }

    pub fn seed_decimal(seed: &str) -> Result<Self, String> {
        let digits = seed
            .strip_prefix('-')
            .or_else(|| seed.strip_prefix('+'))
            .unwrap_or(seed);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Seed must be an integer".into());
        }
        let mut words = vec![0u32];
        for digit in digits.bytes() {
            let mut carry = (digit - b'0') as u64;
            for word in &mut words {
                let value = (*word as u64) * 10 + carry;
                *word = value as u32;
                carry = value >> 32;
            }
            if carry != 0 {
                words.push(carry as u32);
            }
        }
        Ok(Self::seed_words(&words))
    }

    fn seed_words(keys: &[u32]) -> Self {
        let mut result = Self {
            words: [0; 624],
            index: 624,
            gauss: None,
        };
        result.words[0] = 19650218;
        for i in 1..624 {
            result.words[i] = 1812433253u32
                .wrapping_mul(result.words[i - 1] ^ (result.words[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..624.max(keys.len()) {
            result.words[i] = (result.words[i]
                ^ (result.words[i - 1] ^ (result.words[i - 1] >> 30)).wrapping_mul(1664525))
            .wrapping_add(keys[j])
            .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                result.words[0] = result.words[623];
                i = 1;
            }
            if j >= keys.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            result.words[i] = (result.words[i]
                ^ (result.words[i - 1] ^ (result.words[i - 1] >> 30)).wrapping_mul(1566083941))
            .wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                result.words[0] = result.words[623];
                i = 1;
            }
        }
        result.words[0] = 0x80000000;
        result
    }

    pub fn from_state(state: &RngState) -> Self {
        Self {
            words: state.words,
            index: state.index,
            gauss: state.gauss,
        }
    }

    pub fn state(&self) -> RngState {
        RngState {
            version: 3,
            words: self.words,
            index: self.index,
            gauss: self.gauss,
            raw_words: None,
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.index >= 624 {
            for i in 0..624 {
                let y = (self.words[i] & 0x80000000) | (self.words[(i + 1) % 624] & 0x7fffffff);
                self.words[i] = self.words[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.words[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^= y >> 18;
        y
    }

    pub fn getrandbits(&mut self, bits: u32) -> u64 {
        assert!(bits <= 64);
        match bits {
            0 => 0,
            1..=32 => (self.next_u32() >> (32 - bits)) as u64,
            _ => self.next_u32() as u64 | ((self.next_u32() >> (64 - bits)) as u64) << 32,
        }
    }

    pub fn random(&mut self) -> f64 {
        let a = self.next_u32() >> 5;
        let b = self.next_u32() >> 6;
        (a as f64 * 67108864.0 + b as f64) / 9007199254740992.0
    }

    pub fn randbelow(&mut self, n: usize) -> usize {
        assert!(n > 0, "Cannot choose from an empty range");
        let bits = usize::BITS - n.leading_zeros();
        loop {
            let value = self.getrandbits(bits) as usize;
            if value < n {
                return value;
            }
        }
    }

    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            let j = self.randbelow(i + 1);
            values.swap(i, j);
        }
    }
}
