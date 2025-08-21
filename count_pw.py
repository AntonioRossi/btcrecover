import sys

def human_readable_size(size_in_bytes):
    """Convert bytes to human-readable format (e.g., KB, MB, GB, TB)."""
    for unit in ['B', 'KB', 'MB', 'GB', 'TB', 'PB']:
        if size_in_bytes < 1024:
            return f"{size_in_bytes:.2f} {unit}"
        size_in_bytes /= 1024
    return f"{size_in_bytes:.2f} PB"

def main():
    # Get inputs from user
    try:
        n = int(input("Enter the number of characters (character set size): "))
        if n <= 0:
            raise ValueError("Character set size must be positive.")
        
        l = int(input("Enter the password length: "))
        if l <= 0:
            raise ValueError("Password length must be positive.")
    except ValueError as e:
        print(f"Invalid input: {e}")
        sys.exit(1)

    # Calculate number of passwords: n ** l
    num_passwords = n ** l

    # Assume each password is l bytes + 1 byte for newline
    bytes_per_entry = l + 1
    total_bytes = num_passwords * bytes_per_entry

    # Output results
    print(f"Number of passwords: {num_passwords:,}")
    print(f"Estimated file size: {human_readable_size(total_bytes)}")

if __name__ == "__main__":
    main()