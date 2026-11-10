// SPDX-License-Identifier: Apache-2.0
// Test-only adapter. All parsing and code validation is performed by uwvm2-ros.
// Imports are type-checked without resolving providers or instantiating guests.
#include <uwvm2/uwvm/wasm/feature/impl.h>
#include <uwvm2/validation/standard/wasm3/impl.h>
#include <uwvm2/extension/memtag/globals.h>
#include <cstdio>
#include <fstream>
#include <limits>
#include <string_view>
#include <vector>

namespace feature = uwvm2::uwvm::wasm::feature;
namespace validator = uwvm2::validation::standard::wasm3;
namespace base = uwvm2::parser::wasm::base;
namespace core = uwvm2::parser::wasm::standard::wasm1::features;
namespace next = uwvm2::parser::wasm::standard::wasm1p1::features;
namespace operation = uwvm2::parser::wasm::concepts::operation;
namespace memtag = uwvm2::extension::memtag;
using first = core::wasm1;
using second = next::wasm1p1;

template<typename T>
auto const& section(feature::wasm_binfmt_ver1_module_storage_t const& module) {
    return operation::get_first_type_in_tuple<T>(module.sections);
}

bool check_globals(feature::wasm_binfmt_ver1_module_storage_t const& module, bool disabled) {
    auto const& imported = section<core::import_section_storage_t<first, second>>(module).importdesc.index_unchecked(2);
    auto const& local = section<core::memory_section_storage_t<first, second>>(module).memories;
    auto memory_bytes = [&](std::uint32_t index, std::uint64_t& bytes) noexcept {
        if (index >= imported.size() + local.size()) return false;
        auto const& memory = index < imported.size()
            ? imported.index_unchecked(index)->imports.storage.memory
            : local.index_unchecked(index - imported.size());
        // This adapter checks static minimum allocation bounds, not runtime
        // owner identity. Imported-memory aliases require an actual linker.
        if (memory.limits.min > (std::numeric_limits<std::uint64_t>::max() >> 16)) return false;
        bytes = static_cast<std::uint64_t>(memory.limits.min) << 16;
        return true;
    };
    bool seen = false;
    for (auto const& custom : section<core::custom_section_storage_t>(module).customs) {
        if (custom.custom_name == memtag::obsolete_globals_section_name) return false;
        if (custom.custom_name != memtag::linear_globals_section_name) continue;
        if (disabled || seen) return false;
        seen = true;
        auto begin = reinterpret_cast<std::byte const*>(custom.custom_begin);
        auto end = reinterpret_cast<std::byte const*>(custom.sec_span.sec_end);
        std::vector<memtag::global_tag_range> ranges;
        auto result = memtag::visit_linear_globals(
            std::span<std::byte const>(begin, end), memory_bytes,
            [&](memtag::linear_global_descriptor descriptor) {
                if (descriptor.byte_length) {
                    ranges.push_back({static_cast<std::uintptr_t>(descriptor.memory_index),
                        descriptor.address / 16,
                        (descriptor.address + descriptor.byte_length - 1) / 16});
                }
            });
        if (!result || !memtag::validate_global_tag_ranges(ranges)) return false;
    }
    return true;
}

int main(int argc, char** argv) {
    feature::wasm_binfmt_ver1_feature_parameter_storage_t features{};
    auto& flags = feature::wasm_binfmt_ver1_wasm1p1_parameter(features);
    char const* path = nullptr;
    for (int index = 1; index < argc; ++index) {
        std::string_view arg(argv[index]);
        if (arg == "--mode" && index + 1 < argc && std::string_view(argv[index + 1]) == "validation") {
            ++index;
            continue;
        }
        if (arg == "--run" && index + 1 < argc && !path) {
            path = argv[++index];
            continue;
        }
        bool disable = arg.starts_with("--wasm-feature-disable-");
        if (disable) arg.remove_prefix(23);
        else if (arg.starts_with("--wasm-feature-enable-")) arg.remove_prefix(22);
        else { std::fprintf(stderr, "Unknown argument: %s\n", argv[index]); return 2; }
#define SWITCH(name, field) if (arg == name) { flags.field = disable; continue; }
        SWITCH("multi-value", disable_multi_value)
        SWITCH("reference-types", disable_reference_types)
        SWITCH("table-instructions", disable_table_instructions)
        SWITCH("multiple-tables", disable_multiple_tables)
        SWITCH("bulk-memory", disable_bulk_memory)
        SWITCH("sign-extension", disable_sign_extension)
        SWITCH("nontrapping-float-to-int", disable_nontrapping_float_to_int)
        SWITCH("simd", disable_simd)
        SWITCH("extended-const", disable_extended_const)
        SWITCH("table-initializer", disable_table_initializer)
        SWITCH("relaxed-simd", disable_relaxed_simd)
        SWITCH("multi-memory", disable_multi_memory)
        SWITCH("tail-call", disable_tail_call)
        SWITCH("memory64", disable_memory64)
        SWITCH("table64", disable_table64)
        SWITCH("function-references", disable_function_references)
        SWITCH("gc", disable_gc)
        SWITCH("exceptions", disable_exceptions)
        SWITCH("threads", disable_threads)
        SWITCH("memtag-extension", disable_memtag_extension)
#undef SWITCH
        std::fprintf(stderr, "Unknown feature: %s\n", argv[index]);
        return 2;
    }
    if (!path) { std::fputs("Expected --run FILE\n", stderr); return 2; }
    std::ifstream input(path, std::ios::binary | std::ios::ate);
    auto length = input.tellg();
    if (!input || length < 8 || length > 64 * 1024 * 1024) return 2;
    std::vector<std::byte> bytes(static_cast<std::size_t>(length));
    input.seekg(0);
    if (!input.read(reinterpret_cast<char*>(bytes.data()), length)) return 2;
    base::error_impl parse{};
    feature::wasm_binfmt_ver1_module_storage_t module{};
    try { module = feature::binfmt_ver1_handler(bytes.data(), bytes.data() + bytes.size(), parse, features); }
    catch (fast_io::error const&) { std::fputs("uwvm parser threw an error\n", stderr); return 1; }
    if (parse.err_code != base::wasm_parse_error_code::ok) {
        std::fprintf(stderr, "uwvm parser error %u\n", static_cast<unsigned>(parse.err_code));
        return 1;
    }
    auto const& codes = section<core::code_section_storage_t<first, second>>(module).codes;
    auto imported = section<core::import_section_storage_t<first, second>>(module).importdesc.index_unchecked(0).size();
    for (std::size_t index = 0; index < codes.size(); ++index) {
        auto const& code = codes.index_unchecked(index).body;
        uwvm2::validation::error::code_validation_error_impl error{};
        try {
            validator::validate_code(validator::wasm3_code_version{}, module, imported + index,
                reinterpret_cast<std::byte const*>(code.expr_begin),
                reinterpret_cast<std::byte const*>(code.code_end), error, features);
        } catch (fast_io::error const&) {
            std::fprintf(stderr, "uwvm function %zu validation threw an error\n", imported + index);
            return 1;
        }
        if (error.err_code != uwvm2::validation::error::code_validation_error_code::ok) {
            std::fprintf(stderr, "uwvm function %zu validation error %u\n", imported + index,
                static_cast<unsigned>(error.err_code));
            return 1;
        }
    }
    if (!check_globals(module, flags.disable_memtag_extension)) {
        std::fputs("uwvm memtag globals validation failed\n", stderr);
        return 1;
    }
    std::puts("PASS uwvm production parser, all function bodies, and static memtag globals");
}
