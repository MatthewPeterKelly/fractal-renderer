use iter_num_tools::lin_space;
use more_asserts::{assert_ge, assert_gt};

pub struct LookupTable<T: Clone> {
    table_entries: Vec<T>,
    query_offset: f32,
    quear_to_index_scale: f32,
}

impl<T: Clone> LookupTable<T> {
    pub fn new<F>(query_domain: [f32; 2], entry_count: usize, query_to_data: F) -> LookupTable<T>
    where
        F: Fn(f32) -> T,
    {
        assert_ge!(query_domain[1], query_domain[0]);
        assert_gt!(entry_count, 0);

        let queries = lin_space(query_domain[0]..=query_domain[1], entry_count);
        let mut table_entries: Vec<T> = Vec::with_capacity(entry_count);
        for query in queries {
            table_entries.push(query_to_data(query));
        }

        let quear_to_index_scale = (entry_count as f32) / (query_domain[1] - query_domain[0]);

        LookupTable {
            table_entries,
            query_offset: query_domain[0],
            quear_to_index_scale,
        }
    }

    /**
     * @return the table entry corresponding to the query. Out-of-bound requests will be clamped to the domain of the table.
     */
    pub fn lookup(&self, query: f32) -> T {
        let index = (((query - self.query_offset) * self.quear_to_index_scale) as i32)
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
        let lookup_table = LookupTable::new([2.0, 12.0], 11, query_to_data);

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
    }

}