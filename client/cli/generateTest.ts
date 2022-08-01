const { exec } = require('child_process');

const execute = async (command: string, waitInSec: number) => {
    console.log("Executing: ", command)
    return new Promise((resolve, reject) => {
        exec(
            `ts-node index.ts ${command}`,
            (error: any, stdout: string, stderr: string) => {
                console.log(stdout)
                if (error) {
                    reject(error);
                } else {
                    resolve(stdout);
                }
            });
    })
    .then(() => {
        return wait(waitInSec)
    })
}



const generateTestingData = async () => {
    await execute("register roco --export -o 1-register-roco", 20)
    await execute("submit-headers roco --export -o 2-headers-roco", 10);
    await execute("register pang --export -o 3-register-pang", 10);
    await execute("submit-headers roco --export -o 4-headers-roco", 10);
    await execute("submit-headers pang --export -o 5-headers-pang", 10);
}

const wait = (waitInSecs: number) => {
    console.log(`Waiting ${waitInSecs} seconds!`)
    return new Promise(resolve => setTimeout(resolve, waitInSecs * 1000));
};


generateTestingData()

