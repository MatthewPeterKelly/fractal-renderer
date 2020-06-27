use std::io;

fn read_user_input() -> f64 {
    let mut user_input_buffer = String::new();

    println!("Enter a number...");

    io::stdin()
        .read_line(&mut user_input_buffer)
        .expect("Failed to read user input!");

    user_input_buffer
        .trim()
        .parse()
        .expect("That's not a number!")
}

fn compute_square_root(x: f64) -> f64 {
	x
}

fn main() {
    let x = read_user_input();
    println!("You entered: {}", x);
    let y = compute_square_root(x);
    println!("The square root of {} is {}", x, y);
}
