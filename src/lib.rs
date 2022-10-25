use std::thread; 
use std::sync::{mpsc, Arc, Mutex}; 

/* The std lib provides thread::spawn 
that expects to get some code the thread should run as soon as the thread is created.
It doesn't provide a way to create the threads and have them wait for code sent later.

=> Worker data structure between the ThreadPool and the threads: 
1. Define a Worker struct that holds an id and a JoinHandle<()>.
2. ThreadPool to hold a vector of Worker instances.
3. Define a Worker::new that takes an id number and returns a Worker instance 
that holds the id and a thread spawned with an empty closure.
4. In ThreadPool::new, use the for loop counter to generate an id, 
create a new Worker with that id, and store the worker in the vector.

# Sending requests to threads via channels: 
1. The ThreadPool creates a channel and hold on to the sending side of the channel.
2. Each Worker holds on to the receiving side of the channel.
3. A Job type alias for a trait object
that holds the type of closure that .execute receives.
4. The .execute method sends the job it wants to execute 
down the sending side of the channel. 
5. In its thread, the Worker loops over its receiving side of the channel
and executes the closures of any jobs received. */

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Message>,
} 

struct Worker {
    id: usize, 
    thread: Option<thread::JoinHandle<()>>,
}

// Make threads listen for either a Job to run or a signal to stop listening.
enum Message {
    NewJob(Job),
    Terminate, 
}

// Job type alias is a Box of anything that implements the FnBox trait, etc 
// => it's a trait object: 
type Job = Box<dyn FnBox + Send + 'static>; 
    
// # Trick to take ownership of the value inside a Box<T> using self: Box<Self>.
trait FnBox {
    fn call_box(self: Box<Self>);       
}
    
// Implement the FnBox trait for any type F that implements the FnOnce() trait.
impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)()    
        // to take ownership of self and move the value out of the Box.
        // any FnOnce() closure can use .call_box() to move the closure out of the Box 
        // and call the closure.
    }
}

impl ThreadPool {      
    // # Create a new ThreadPool!
    pub fn new (size: usize) -> ThreadPool {        
    // size is the number of the threads in the pool.
        
        assert!(size > 0);
        // panics if the size is zero. 
        
        let (sender, receiver) = mpsc::channel(); 

        let receiver = Arc::new(Mutex::new(receiver)); 

        let mut workers = Vec::with_capacity(size); 

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
            // the workers can share ownership of the receiving end.
        }

        ThreadPool {
            workers,
            sender,
        }
    }

    // Using the thread::spawn impl as a reference: 
    pub fn execute<F>(&self, f: F)
        where
            F: FnOnce() + Send + 'static        
            // () after FnOnce because the closure takes no parameters.
    {
        let job = Box::new(f);

        self.sender.send(Message::NewJob(job)).unwrap();    
    }            
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
    // Mutex<T> ensures that only one Worker thread at a time is trying to request a job.
        
        let thread = thread::spawn(move|| {
        // closure loops forever,
        // asking the receiving end of the channel for a job and running it.
                loop {
                    let message = receiver.lock().unwrap().recv().unwrap();  
                    // .lock() on the receiver to acquire the mutex.
                    //  (can fail if the mutex is in a poisoned state 
                    // (other thread panicked while holding the lock).
                    // .recv() to receive a Job from the channel 
                    // (blocks if there's no job yet, and waits. It can fail if thread 
                    // holding the sending side has shut down, idem viceversa).
                    
                    match message {
                        Message::NewJob(job) => {
                            println! ("Worker {} got a job; executing.", id); 
                            job.call_box(); 
                        },
                        Message::Terminate => {
                            println!("Worker {} was told to terminate.", id);
                            break;
                        },
                    }
                }
        });

        Worker {
            id: id, 
            thread: Some(thread), 
        }
    }
}

// # Graceful shutdown! 
// Impl Drop trait to call .join() on each thread in the pool and clean it up. 
// Threads can finish the requests they're working on before closing.

impl Drop for ThreadPool {
    fn drop(&mut self) {
        println!("Sending terminate message to all workers.");
        for _ in &mut self.workers {
            self.sender.send(Message::Terminate).unwrap();
        }

        println!("Shutting down all workers.");
        for worker in &mut self.workers {
            println!("Shutting down worker {}", worker.id); 

            if let Some(thread) = worker.thread.take() {     
            // .take() on Option to move thread out of worker. 
                thread.join().unwrap();     
                // .join() takes ownership / consumes the thread. 
            }
        }
    }
}