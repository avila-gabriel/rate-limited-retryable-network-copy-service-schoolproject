
//function to log client that accepts arguments like the client id (from 0..n) and the name of the file (a-z)
//now we run the test by:
//create and log the server
//if server log from the line `debug_println!("GET: Sent {} bytes. Total sent: {} / {}", bytes_read, total_sent, remaining);` is
//not equal to whats suppose to be we panic.
//now we create a bunch of files and a bunch of clients and have them to make a get operation and we too check their receiving just like in the server log as by the line:
//`debug_println!("Received {} bytes. Total received: {} / {}", bytes_read, received, remaining_size);`
//
//
//
//
//
//
//