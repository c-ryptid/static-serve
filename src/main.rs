use static_serve::ThreadPool;
use std::fs;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::str;
use std::env::args;

fn main() {
    // Defaults
    let mut temp_ipv4addr: String = String::from("127.0.0.1:8080");
    let mut temp_rootpath: String = String::from(".");
    let mut temp_indexfile: String = String::from("index.html");
    let mut temp_error404file: String = String::from("");
    let mut temp_poolsize: usize = 1;
    
    // CLI argument handling
    let mut args = args().skip(1);
    while let Some(arg) = args.next() {
        match &arg[..] {
            "-h" | "--help" => {
                printhelp();
                return;
            },
            "-u" | "--usage" => {
                printusagehelp();
                return;
            },
            "-a" | "--address" => {
                if let Some(arg_ipv4addr) = args.next() {
                    temp_ipv4addr = arg_ipv4addr;
                } else {
                    panic!("No value specified for parameter --address.");
                }
            }
            "-r" | "--rootpath" => {
                if let Some(arg_rootpath) = args.next() {
                    temp_rootpath = arg_rootpath;
                } else {
                    panic!("No value specified for parameter --rootpath.");
                }
            }
            "-i" | "--indexfile" => {
                if let Some(arg_indexfile) = args.next() {
                    temp_indexfile = arg_indexfile;
                } else {
                    panic!("No value specified for parameter --indexfile.");
                }
            }
            "-e" | "--error404file" => {
                if let Some(arg_error404file) = args.next() {
                    temp_error404file = arg_error404file;
                }
            }
            "-p" | "--poolsize" => {
                if let Some(arg_poolsize) = args.next() {
                    temp_poolsize = match arg_poolsize.parse::<usize>() {
                        Ok(size) => size,
                        Err(_) => {
                            panic!("Invalid value specified for parameter --poolsize.");
                        },
                    };
                } else {
                    panic!("No value specified for parameter --poolsize.");
                }
            }
            _ => {
                if arg.starts_with('-') {
                    println!("Unkown argument {}", arg);
                } else {
                    println!("Unkown positional argument {}", arg);
                }
            }
        }
    }

    // Get final state
    let ipv4addr: &str = Box::leak(Box::new(temp_ipv4addr));
    let rootpath: &str = Box::leak(Box::new(temp_rootpath));
    let indexfile: &str = Box::leak(Box::new(temp_indexfile));
    let error404file: &str = Box::leak(Box::new(temp_error404file));
    let poolsize = temp_poolsize;

    // 404 file and directory validation
    if error404file != ""{
        if entrycheck(&rootpath, &error404file) {
            println!("you need a valid 404 file");
            return;
        }
    }

    // Bind to address
    let listener = TcpListener::bind(ipv4addr).unwrap();
    // Create a thread pool
    let pool = ThreadPool::new(poolsize);
    
    // Main server loop
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        // Dispatch thread to handle connection
        pool.execute(|| {
            handle_connection(stream, rootpath, indexfile, error404file);
        });
    }
}

// This function checks if the request is a GET request and waits for handle_get_request()
// to return the response. Then writes the response bytes to the stream and closes it.
fn handle_connection(mut stream: TcpStream, rootpath: &str, indexfile: &str, error404file: &str) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap();

    if buffer.starts_with(b"GET ") {
        let response = handle_get_request(buffer, rootpath, indexfile, error404file);

        stream.write_all(response.as_slice()).unwrap();
        stream.flush().unwrap();
    }
}

// This function does all the file handling and status code determination
// If the file can't be read, it attempts to return the contents of the 404 file instead.
// If there is no 404 file, this is skipped 
fn handle_get_request(buffer: [u8; 1024], rootpath: &str, indexfile: &str, error404file: &str) -> Vec<u8> {
    let uri = get_uri(buffer, indexfile);

    let found;
    let mut contents = match fs::read(format!("{}{}", rootpath, uri)) {
        Ok(file) => {
            found = "200 OK";
            file 
        },
        Err(_) => {
            found = "404 NOT FOUND";
            "".as_bytes().to_vec()
        },
    };

    // Ignore if there is no 404 file
    if error404file != "" {
        // If the file contents are empty, replace them with the 404 file
        // If the 404 file fails to read, the file contents remain empty
        if contents == "".as_bytes().to_vec() {
            contents = match fs::read(format!("{}/{}", rootpath, error404file)) {
                Ok(file) => file,
                Err(_) => "".as_bytes().to_vec(),
            };
            
        }
    }
    
    // Get the proper status line
    let status_line = format!("HTTP/1.1 {}",found);

    // Format properly for a HTTP/1.1 response
    let response = format!(
        "{}\r\nContent-Length: {}\r\n\r\n",
        status_line,
        contents.len()
    );

    // Concatenate the response and file contents and return them as bytes
    let responsebytes = [response.as_bytes(), &contents].concat();
    return responsebytes;
}

// This function reads the request buffer into a uri and appends the index file
// if the uri ends with a '/' character, which signifies a directory.
fn get_uri(buffer: [u8; 1024], indexfile: &str) -> String {
    let mut requesteduri = [0; 256];
    let mut readuri: bool = false;
    let mut urireads: usize = 0;
    for i in 0..256{
        // 32 here is a whitespace character.
        // This means that if a whitespace is encountered
        // for the first time, we start reading the buffer
        // to ultimately get the whole uri.
        // We stop reading and break out of the loop once
        // we hit a second whitespace 
        if buffer[i] == 32 {
            if readuri {
                break;
            }
            readuri = true;
            continue;
        }
        
        if readuri {
            requesteduri[urireads] = buffer[i];
            urireads += 1;
        }
    }

    // Make the u8 array into a &str
    let filename = match str::from_utf8(&requesteduri){
        Ok(v) => v.trim_matches(char::from(0)),
        Err(_) => "",
    }; 
    
    let finalfileuri;
    if filename.ends_with("/") {
        finalfileuri = [filename, &indexfile].concat().to_string();
    } else {
        finalfileuri = filename.to_string();
    }
    return finalfileuri;
}

// Takes in a path and file 
// Returns true if it fails to read a file of the resulting URI
// Returns false if it succeeds
fn entrycheck(path: &str, file: &str) -> bool{
    let _contents = match fs::read(format!("{}/{}", path, file)) {
        Ok(_file) => return false,
        Err(_) => return true,
    };
}

// Prints general help
fn printhelp() {
    print!("Usage: binary [OPTIONS]
Start a HTTP/1.1 server bound to a specified IPv4 address and serve
content from the document root.

Options:
  -h, --help                  display this help and exit

  -u, --usage                 display option usage help and exit

  -a, --address=ADDRESS       set the IPv4 address and port that the server
                              tries to bind to
                              [default=127.0.0.1:8080]

  -r, --rootpath=FILE         set the path to the document root
                              [default=.]

  -i, --indexfile=FILE        set the default index file for whenever a user
                              accesses a directory instead of a resource
                              [default=index.html]

  -e, --error404file[=FILE]   set the location of the 404 file in relation to
                              the document root, disables 404 pages without value
                              [default=404.html]

  -p, --poolsize=SIZE         set the thread pool size, recommended to keep
                              this value under the core count of the server
                              [default=1]
        
");
    
}

// Prints examples
fn printusagehelp() {
print!("The following is a small guide to option usage with examples

-a, --address=ADDRESS means the value is mandatory

  Valid options:
    -a 127.0.0.1:8080
    --address 127.0.0.1:8080

  Invalid options:
    -a=127.0.0.1:8080
    --address 127.0.0.1
    -address 127.0.0.1:8080
    -a \"127.0.0.1:8080\"
    -a

-e, --error404file[=FILE] means the value is optional

  Valid options:
    -e 404.html
    -e
    --error404file

  Invalid options:
    -e /404.html
    -e \"404.html\"

");
}
