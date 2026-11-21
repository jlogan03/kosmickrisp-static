use ash::{Device, Entry, Instance, vk};
use rspirv::{binary::Assemble, dr::Builder, dr::Operand, spirv};
use std::{error::Error, ffi::CStr};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ELEMENTS: u32 = 128;

// Each invocation replaces one input with input * 3 + 1.
fn shader() -> Vec<u32> {
    let mut b = Builder::new();
    b.set_version(1, 3);
    b.capability(spirv::Capability::Shader);
    b.memory_model(spirv::AddressingModel::Logical, spirv::MemoryModel::GLSL450);
    let void = b.type_void();
    let uint = b.type_int(32, 0);
    let vector = b.type_vector(uint, 3);
    let input = b.type_pointer(None, spirv::StorageClass::Input, vector);
    let global_id = b.variable(input, None, spirv::StorageClass::Input, None);
    b.decorate(
        global_id,
        spirv::Decoration::BuiltIn,
        [Operand::BuiltIn(spirv::BuiltIn::GlobalInvocationId)],
    );
    let array = b.type_runtime_array(uint);
    b.decorate(
        array,
        spirv::Decoration::ArrayStride,
        [Operand::LiteralBit32(size_of::<u32>() as u32)],
    );
    let block = b.type_struct([array]);
    b.decorate(block, spirv::Decoration::Block, []);
    b.member_decorate(
        block,
        0,
        spirv::Decoration::Offset,
        [Operand::LiteralBit32(0)],
    );
    let storage = b.type_pointer(None, spirv::StorageClass::StorageBuffer, block);
    let element = b.type_pointer(None, spirv::StorageClass::StorageBuffer, uint);
    let buffer = b.variable(storage, None, spirv::StorageClass::StorageBuffer, None);
    b.decorate(
        buffer,
        spirv::Decoration::DescriptorSet,
        [Operand::LiteralBit32(0)],
    );
    b.decorate(
        buffer,
        spirv::Decoration::Binding,
        [Operand::LiteralBit32(0)],
    );
    let zero = b.constant_bit32(uint, 0);
    let one = b.constant_bit32(uint, 1);
    let three = b.constant_bit32(uint, 3);
    let function = b.type_function(void, []);
    let main = b
        .begin_function(void, None, spirv::FunctionControl::NONE, function)
        .unwrap();
    b.begin_block(None).unwrap();
    let id = b.load(vector, None, global_id, None, []).unwrap();
    let index = b.composite_extract(uint, None, id, [0]).unwrap();
    let ptr = b
        .access_chain(element, None, buffer, [zero, index])
        .unwrap();
    let value = b.load(uint, None, ptr, None, []).unwrap();
    let product = b.i_mul(uint, None, value, three).unwrap();
    let result = b.i_add(uint, None, product, one).unwrap();
    b.store(ptr, result, None, []).unwrap();
    b.ret().unwrap();
    b.end_function().unwrap();
    b.entry_point(spirv::ExecutionModel::GLCompute, main, "main", [global_id]);
    b.execution_mode(main, spirv::ExecutionMode::LocalSize, [1, 1, 1]);
    b.module().assemble()
}

// The probe owns every Vulkan object until the submitted work completes.
unsafe fn compute(
    instance: &Instance,
    physical: vk::PhysicalDevice,
    device: &Device,
    family: u32,
) -> Result<()> {
    unsafe {
        let bytes = ELEMENTS as u64 * size_of::<u32>() as u64;
        let buffer = device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(bytes)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER),
            None,
        )?;
        let requirements = device.get_buffer_memory_requirements(buffer);
        let properties = instance.get_physical_device_memory_properties(physical);
        let flags = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let memory_type = (0..properties.memory_type_count)
            .find(|&i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && properties.memory_types[i as usize]
                        .property_flags
                        .contains(flags)
            })
            .ok_or("no coherent host-visible memory")?;
        let memory = device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )?;
        device.bind_buffer_memory(buffer, memory, 0)?;
        let mapped = device
            .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?
            .cast::<u32>();
        for i in 0..ELEMENTS {
            mapped.add(i as usize).write(i);
        }

        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        let set_layout = device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&binding),
            None,
        )?;
        let set_layouts = [set_layout];
        let layout = device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
            None,
        )?;
        let code = shader();
        let module = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(c"main");
        let pipeline = device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(stage)
                    .layout(layout)],
                None,
            )
            .map_err(|(_, e)| e)?[0];
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)];
        let descriptors = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&sizes),
            None,
        )?;
        let set = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptors)
                .set_layouts(&set_layouts),
        )?[0];
        let buffers = [vk::DescriptorBufferInfo::default()
            .buffer(buffer)
            .range(bytes)];
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&buffers)],
            &[],
        );
        let pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(family),
            None,
        )?;
        let commands = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?;
        let command = commands[0];
        device.begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())?;
        device.cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, pipeline);
        device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::COMPUTE,
            layout,
            0,
            &[set],
            &[],
        );
        device.cmd_dispatch(command, ELEMENTS, 1, 1);
        let barrier = [vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ)];
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &barrier,
            &[],
            &[],
        );
        device.end_command_buffer(command)?;
        let queue = device.get_device_queue(family, 0);
        let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(&commands)],
            fence,
        )?;
        device.wait_for_fences(&[fence], true, 30_000_000_000)?; // 30 seconds.
        for i in 0..ELEMENTS {
            assert_eq!(mapped.add(i as usize).read(), i * 3 + 1, "element {i}");
        }
        device.unmap_memory(memory);
        device.destroy_fence(fence, None);
        device.destroy_command_pool(pool, None);
        device.destroy_descriptor_pool(descriptors, None);
        device.destroy_pipeline(pipeline, None);
        device.destroy_shader_module(module, None);
        device.destroy_pipeline_layout(layout, None);
        device.destroy_descriptor_set_layout(set_layout, None);
        device.destroy_buffer(buffer, None);
        device.free_memory(memory, None);
        println!("Compute passed: {ELEMENTS} values match input * 3 + 1");
        Ok(())
    }
}

fn probe(entry: Entry, instance_only: bool) -> Result<()> {
    unsafe {
        assert!(entry.enumerate_instance_layer_properties()?.is_empty());
        let info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
        let instance = entry.create_instance(
            &vk::InstanceCreateInfo::default().application_info(&info),
            None,
        )?;
        let devices = instance.enumerate_physical_devices()?;
        println!("Vulkan instance passed: {} GPU(s)", devices.len());
        if instance_only {
            instance.destroy_instance(None);
            return Ok(());
        }
        let physical = *devices.first().ok_or("KosmicKrisp found no GPU")?;
        let properties = instance.get_physical_device_properties(physical);
        println!(
            "Device: {}",
            CStr::from_ptr(properties.device_name.as_ptr()).to_string_lossy()
        );
        let family = instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .ok_or("no compute queue")? as u32;
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&[1.0])];
        let device = instance.create_device(
            physical,
            &vk::DeviceCreateInfo::default().queue_create_infos(&queues),
            None,
        )?;
        let result = compute(&instance, physical, &device, family);
        device.device_wait_idle()?;
        device.destroy_device(None);
        instance.destroy_instance(None);
        result
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<()> {
    unsafe extern "system" fn lookup(
        instance: vk::Instance,
        name: *const core::ffi::c_char,
    ) -> vk::PFN_vkVoidFunction {
        use ash::vk::Handle;
        unsafe { kosmickrisp_static::get_instance_proc_addr(instance.as_raw() as *mut _, name) }
    }
    let entry = unsafe {
        Entry::from_static_fn(ash::StaticFn {
            get_instance_proc_addr: lookup,
        })
    };
    let instance_only = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--instance-only" => true,
        _ => return Err("usage: probe [--instance-only]".into()),
    };
    probe(entry, instance_only)
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    // Keep the shared probe code type-checked on development hosts.
    let _ = probe;
    eprintln!("This probe requires ARM64 macOS.");
    std::process::exit(1);
}
