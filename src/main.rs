use std::io;

fn main() {
    println!("Hello, world!");
    let mut user_input = String::new();

    io::stdin().read_line(&mut user_input).expect("Failed to read user input!");

    println!("You entered: {}", user_input.trim());

}