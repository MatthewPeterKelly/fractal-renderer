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

fn compute_newton_step(x: f64, c: f64) -> f64 {
    // x = current guess
    // c = number to compute the square root of
    //
    // want to find the solution to x^2 - c = f(x) --> 0
    // f' = 2*x
    // Newton step is:  x -f(x) / f'(x)
    0.5 * (c / x + x)
}

fn compute_square_root(c: f64) -> f64 {
    let mut x = 0.5 * (c + 1.0); // initial guess
    let mut y: f64 = 0.0;
    for iter in 0..25 {
        y = compute_newton_step(x, c);
        println!("iter: {},   x:{},  y:{}", iter,  x, y);
        if (x-y).abs() < 1e-12 {
        	break
        }
        x = y;
    }
    y
}

fn main() {
    let x = read_user_input();
    println!("Let's compute the square root of {} using Newtons Method...", x);
    let y = compute_square_root(x);
    println!("The square root of {} is {}", x, y);
}
