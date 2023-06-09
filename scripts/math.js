export function get_power_of_10(num) {
    let power = 0;
    
    while (num % 1 !== 0) {
        num *= 10;
        power += 1;
    }
    
    return power;
}

export function gcd(a, b) {
    if (b === 0) {
        return a;
    }

    return gcd(b, a % b);
}

export function fractional_gcd(a, b) {
    let power = Math.max(get_power_of_10(a), get_power_of_10(b));
    let factor = gcd(a * 10 ** power, b * 10 ** power);

    return factor / (10 ** power);
}
