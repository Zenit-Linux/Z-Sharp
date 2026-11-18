#[derive(Clone, Debug)]
pub struct Config {
    /// Rozmiar pierwszego chunka areny.
    pub chunk_size: usize,
    /// Górna granica wzrostu chunków (chunk rośnie 2x do tej granicy).
    pub max_chunk_size: usize,
    /// Alokacje >= tej wartości trafiają do Large Object Space.
    pub large_object: usize,
    /// Ile chunków arena zatrzymuje po `reset` zamiast oddawać do puli.
    pub retain_chunks: usize,
    /// Maksymalny rozmiar pamięci trzymanej w wolnej puli chunków.
    pub pool_cap_bytes: usize,
    /// Wypełnianie zwolnionej pamięci wzorcem 0xDD (wykrywanie użycia po zwolnieniu).
    pub poison: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { chunk_size: 64 << 10, max_chunk_size: 1 << 20, large_object: 256 << 10, retain_chunks: 4,
                 pool_cap_bytes: 16 << 20, poison: cfg!(debug_assertions) }
    }
}

impl Config {
    /// Ustawia opcję po nazwie z `Z#.toml` (`chunk-kib`, `max-chunk-kib`, `large-object-kib`,
    /// `retain-chunks`, `pool-cap-kib`, `poison`). Zwraca `false` dla nieznanej nazwy lub złej wartości.
    pub fn set(&mut self, key: &str, val: i64) -> bool {
        if val < 0 { return false; }
        let v = val as usize;
        match key {
            "chunk-kib" if v >= 1 => self.chunk_size = v << 10,
            "max-chunk-kib" if v >= 1 => self.max_chunk_size = v << 10,
            "large-object-kib" if v >= 1 => self.large_object = v << 10,
            "retain-chunks" => self.retain_chunks = v,
            "pool-cap-kib" => self.pool_cap_bytes = v << 10,
            "poison" => self.poison = v != 0,
            _ => return false,
        }
        if self.max_chunk_size < self.chunk_size { self.max_chunk_size = self.chunk_size; }
        true
    }
}
