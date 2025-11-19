#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
mod measured_module {
    pub fn function_one() {
        let vec = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
        std::hint::black_box(&vec);
    }

    pub fn function_two() {
        let vec = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
        std::hint::black_box(&vec);
    }

    pub fn function_three() {}
}

#[cfg_attr(feature = "hotpath", hotpath::main(limit = 3))]
fn main() {
    for _ in 0..10 {
        measured_module::function_one();
        measured_module::function_two();
    }
    measured_module::function_three();
}
