use std::io;

fn main() {
    let mut user_input_buffer = String::new();

    println!("Enter a number...");

    io::stdin()
        .read_line(&mut user_input_buffer)
        .expect("Failed to read user input!");

    let user_input: f64 = user_input_buffer
        .trim()
        .parse()
        .expect("That's not a number!");

    println!("You entered: {}", user_input);
}
