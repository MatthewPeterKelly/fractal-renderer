use iter_num_tools::lin_space;
use more_asserts::assert_ge;

pub struct LookupTable<T> {
    table_entries: Vec<T>,
    query_offset: f32,
    quear_to_index_scale: f32,
}

impl<T> LookupTable<T> {
    pub fn new<F>(query_domain: [f32; 2], entry_count: usize, query_to_data: F) -> LookupTable<T>
    where
        F: Fn(f32) -> T,
    {
        assert_ge!(query_domain[1], query_domain[0]);

        let queries = lin_space(query_domain[0]..=query_domain[1], entry_count);
        let mut table_entries: Vec<T> = Vec::new();
        // TODO:  reserve correct size?
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
}
