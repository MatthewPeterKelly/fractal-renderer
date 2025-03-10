use iter_num_tools::lin_space;

#[derive(Default)]
pub struct LookupTable<T: Clone> {
    table_entries: Vec<T>,
    query_offset: f32,
    query_to_index_scale: f32,
}

impl<T: Clone> LookupTable<T> {
    /// Allocates a new table and populates it with values from a lambda function
    pub fn new<F>(query_domain: [f32; 2], entry_count: usize, query_to_data: F) -> LookupTable<T>
    where
        F: Fn(f32) -> T,
    {
        assert!(entry_count > 0);
        let nominal_value = query_to_data(0.5 * (query_domain[0] + query_domain[1]));
        let mut lookup_table = LookupTable {
            table_entries: vec![nominal_value; entry_count],
            query_offset: 0.0,
            query_to_index_scale: 1.0,
        };
        lookup_table.reset(query_domain, query_to_data);
        lookup_table
    }

    /// Updates the table in-place, without allocating. Overwrites all data in the existing table.
    pub fn reset<F>(&mut self, query_domain: [f32; 2], query_to_data: F)
    where
        F: Fn(f32) -> T,
    {
        assert!(query_domain[1] > query_domain[0]);

        let entry_count = self.table_entries.len();
        let queries = lin_space(query_domain[0]..=query_domain[1], entry_count);

        for (i, query) in queries.enumerate() {
            self.table_entries[i] = query_to_data(query);
        }

        self.query_offset = query_domain[0];
        self.query_to_index_scale = (entry_count as f32) / (query_domain[1] - query_domain[0]);
    }

    /// @return the table entry corresponding to the query. Out-of-bound requests will be clamped to the domain of the table.
    pub fn lookup(&self, query: f32) -> T {
        let index = (((query - self.query_offset) * self.query_to_index_scale) as i32)
            .clamp(0, self.table_entries.len() as i32 - 1);
        self.table_entries[index as usize].clone()
    }
}

#[cfg(test)]
mod tests {

    use super::LookupTable;

    #[test]
    fn test_lookup_table() {
        // Define a query to data function
        let query_to_data = |x: f32| (x * 2.0) as i32;

        // Create a LookupTable with a query domain from 0.0 to 10.0 and 11 entries
        let mut lookup_table = LookupTable::new([2.0, 12.0], 11, query_to_data);

        // Check the length of the table_entries vector
        assert_eq!(lookup_table.table_entries.len(), 11);

        // Test lookup method for in-bound queries
        assert_eq!(lookup_table.lookup(2.0), 4);
        assert_eq!(lookup_table.lookup(5.0), 10);
        assert_eq!(lookup_table.lookup(12.0), 24);

        // Test lookup method for out-of-bound queries
        assert_eq!(lookup_table.lookup(0.0), 4);
        assert_eq!(lookup_table.lookup(-2.5), 4);
        assert_eq!(lookup_table.lookup(15.0), 24);

        // Check that the reset logic works
        // Define a query to data function
        let query_to_data = |x: f32| (1.0 + x * 3.0) as i32;
        lookup_table.reset([3.0, 5.0], query_to_data);

        // Test lookup method for in-bound queries
        assert_eq!(lookup_table.lookup(3.0), 10);
        assert_eq!(lookup_table.lookup(4.0), 13);
        assert_eq!(lookup_table.lookup(5.0), 16);

        // Test lookup method for out-of-bound queries
        assert_eq!(lookup_table.lookup(0.0), 10);
        assert_eq!(lookup_table.lookup(-2.5), 10);
        assert_eq!(lookup_table.lookup(15.0), 16);
    }
}
