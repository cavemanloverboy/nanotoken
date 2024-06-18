import argparse
from collections import defaultdict


def parse_log_file(file_path):
    # Initialize a dictionary to hold slot counts using defaultdict to automatically handle new keys
    slot_counts = defaultdict(int)

    # Open and read the log file
    with open(file_path, 'r') as file:
        for line in file:
            if "Transaction executed in slot" in line:
                # Extract the slot number from the line
                slot_number = line.split()[-1]
                # Increment the count for this slot in the dictionary
                slot_counts[slot_number] += 1

    return slot_counts


def print_histogram(slot_counts):
    print("Slot Histogram:")
    for slot, count in slot_counts.items():
        print(f"Slot {slot}: {count} transactions")


def find_max_slot(slot_counts):
    # Find the slot with the highest count
    max_slot = max(slot_counts, key=slot_counts.get)
    print(
        f"\nSlot with the highest transactions: {max_slot} ({slot_counts[max_slot]} transactions)")


def main():
    # Setup argparse for command line arguments
    parser = argparse.ArgumentParser(
        description="Count transactions by slot in a log file and print a histogram.")
    parser.add_argument("log_file", help="Path to the log file to be analyzed")

    # Parse arguments
    args = parser.parse_args()

    # Parse the log file
    slot_counts = parse_log_file(args.log_file)

    # Print the histogram
    print_histogram(slot_counts)

    # Print the slot with the highest count
    find_max_slot(slot_counts)


if __name__ == "__main__":
    main()
